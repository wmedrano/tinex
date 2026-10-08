use anyhow::Result;
use jack::PortSpec;
use std::sync::{Arc, mpsc};
use tracing::{error, info, warn};

use crate::tinex::{ProcessArgs, Tinex, TinexNotification, TinexRequest};

/// The returned port handles are valid while the JACK client remains active.
pub struct TinexHandle {
    pub _client: jack::AsyncClient<SharedNotificationHandler, ProcessHandler>,
    pub _notification_handler: Arc<NotificationHandler>,
    pub _requests: mpsc::Sender<TinexRequest>,
    pub notifications: mpsc::Receiver<TinexNotification>,
}

pub fn init_jack(autoconnect: bool) -> Result<TinexHandle> {
    let (client, status) = jack::Client::new("tinex", jack::ClientOptions::default())?;
    info!(?client, "Created JACK client");
    if !status.is_empty() {
        warn!(?status, "Got non-empty JACK status.");
    }

    let (requests, receiver) = mpsc::channel();
    let (notification_sender, notifications) = mpsc::channel();
    let process_handler = ProcessHandler::new(&client, receiver, notification_sender)?;
    let notification_handler = Arc::new(NotificationHandler::new(&process_handler, autoconnect)?);
    let client = client.activate_async(
        SharedNotificationHandler(Arc::clone(&notification_handler)),
        process_handler,
    )?;
    Ok(TinexHandle {
        _client: client,
        _notification_handler: notification_handler,
        _requests: requests,
        notifications,
    })
}

pub struct ProcessHandler {
    tinex: Tinex,
    arena: bumpalo::Bump,
    midi_in: jack::Port<jack::MidiIn>,
    audio_in: [jack::Port<jack::AudioIn>; 2],
    audio_out: [jack::Port<jack::AudioOut>; 2],
}

impl ProcessHandler {
    fn new(
        client: &jack::Client,
        requests: mpsc::Receiver<TinexRequest>,
        notifications: mpsc::Sender<TinexNotification>,
    ) -> Result<ProcessHandler> {
        Ok(ProcessHandler {
            tinex: Tinex::new(requests, notifications),
            arena: bumpalo::Bump::with_capacity(64 * 1024),
            midi_in: client.register_port("midi-in", jack::MidiIn::default())?,
            audio_in: make_ports(client, jack::AudioIn::default(), "audio-in")?,
            audio_out: make_ports(client, jack::AudioOut::default(), "audio-out")?,
        })
    }
}

impl jack::ProcessHandler for ProcessHandler {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        self.arena.reset();
        let input = self.audio_in.each_ref().map(|port| port.as_slice(ps));
        let output = self.audio_out.each_mut().map(|port| port.as_mut_slice(ps));
        let events = self.midi_in.iter(ps);
        let mut messages =
            bumpalo::collections::Vec::with_capacity_in(events.size_hint().0, &self.arena);
        for event in events {
            if let Ok(message) = wmidi::MidiMessage::try_from(event.bytes)
                && let Some(message) = message.drop_unowned_sysex()
            {
                messages.push(message);
            }
        }
        let args = ProcessArgs {
            _input: input,
            _midi_input: messages.into_bump_slice(),
            output,
            _arena: &self.arena,
        };
        self.tinex.process(args);
        jack::Control::Continue
    }
}

pub struct NotificationHandler {
    autoconnect: bool,
    pub midi_in: jack::Port<jack::Unowned>,
    pub audio_in: [jack::Port<jack::Unowned>; 2],
    pub audio_out: [jack::Port<jack::Unowned>; 2],
}

/// Adapts the shared handler to JACK's notification callback trait.
pub struct SharedNotificationHandler(Arc<NotificationHandler>);

impl NotificationHandler {
    fn new(process_handler: &ProcessHandler, autoconnect: bool) -> Result<NotificationHandler> {
        let midi_in = process_handler.midi_in.clone_unowned();
        let audio_in = process_handler
            .audio_in
            .each_ref()
            .map(|port| port.clone_unowned());
        let audio_out = process_handler
            .audio_out
            .each_ref()
            .map(|port| port.clone_unowned());
        Ok(NotificationHandler {
            autoconnect,
            midi_in,
            audio_in,
            audio_out,
        })
    }

    fn autoconnect_port(&self, client: &jack::Client, port: &jack::Port<jack::Unowned>) {
        enum Direction {
            Input,
            Output,
        }

        let Ok(port_type) = port.port_type() else {
            warn!("Could not determine port type for autoconnect");
            return;
        };
        let flags = port.flags();
        if port_type == jack::MidiIn::default().jack_port_type() {
            if client.is_mine(port)
                || flags.contains(jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_OUTPUT)
            {
                self.autoconnect_midi(client);
            }
            return;
        }
        if !client.is_mine(port) {
            return;
        }
        let Ok(local_port_name) = port.name() else {
            warn!("Could not determine port name for autoconnect");
            return;
        };
        let direction = if flags.contains(jack::PortFlags::IS_INPUT) {
            Direction::Input
        } else if flags.contains(jack::PortFlags::IS_OUTPUT) {
            Direction::Output
        } else {
            return;
        };
        let local_ports = match direction {
            Direction::Input => {
                if port_type == jack::AudioIn::default().jack_port_type() {
                    &self.audio_in
                } else {
                    return;
                }
            }
            Direction::Output => {
                if port_type != jack::AudioOut::default().jack_port_type() {
                    return;
                }
                &self.audio_out
            }
        };
        let physical_flags = match direction {
            Direction::Input => jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_OUTPUT,
            Direction::Output => jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_INPUT,
        };
        let physical_port_names = client.ports(None, Some(&port_type), physical_flags);
        for physical_port_name in physical_port_names {
            let is_already_connected = local_ports.iter().any(|local_port| {
                local_port
                    .is_connected_to(&physical_port_name)
                    .unwrap_or(false)
            });
            if is_already_connected {
                continue;
            }
            let (source, destination) = match direction {
                Direction::Input => (&physical_port_name, &local_port_name),
                Direction::Output => (&local_port_name, &physical_port_name),
            };
            match client.connect_ports_by_name(source, destination) {
                Ok(()) => return,
                Err(error) => warn!(
                    ?error,
                    physical_port_name, local_port_name, "Could not autoconnect port"
                ),
            }
        }
    }

    fn autoconnect_midi(&self, client: &jack::Client) {
        let Ok(local_port_name) = self.midi_in.name() else {
            warn!("Could not determine MIDI input name for autoconnect");
            return;
        };
        let physical_port_names = client.ports(
            None,
            Some(jack::MidiIn::default().jack_port_type()),
            jack::PortFlags::IS_PHYSICAL | jack::PortFlags::IS_OUTPUT,
        );
        for physical_port_name in physical_port_names {
            if self
                .midi_in
                .is_connected_to(&physical_port_name)
                .unwrap_or(false)
            {
                continue;
            }
            if let Err(error) = client.connect_ports_by_name(&physical_port_name, &local_port_name)
            {
                warn!(
                    ?error,
                    physical_port_name, local_port_name, "Could not autoconnect MIDI port"
                );
            }
        }
    }
}

impl jack::NotificationHandler for SharedNotificationHandler {
    fn thread_init(&self, _: &jack::Client) {
        info!("JACK initialized thread");
    }

    unsafe fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        error!(?status, reason, "JACK shutdown");
    }

    fn freewheel(&mut self, _: &jack::Client, is_freewheel: bool) {
        info!(is_freewheel, "JACK freewheel changed");
    }

    fn sample_rate(&mut self, _: &jack::Client, sample_rate: jack::Frames) -> jack::Control {
        info!(sample_rate, "JACK sample rate changed");
        jack::Control::Continue
    }

    fn client_registration(&mut self, _: &jack::Client, name: &str, is_registered: bool) {
        if is_registered {
            info!(name, "JACK client registered");
        } else {
            info!(name, "JACK client removed");
        }
    }

    fn port_registration(
        &mut self,
        client: &jack::Client,
        port_id: jack::PortId,
        is_registered: bool,
    ) {
        let Some(port) = client.port_by_id(port_id) else {
            warn!(?port_id, ?is_registered, "Invalid port registration");
            return;
        };
        let port_name = port
            .name()
            .ok()
            .unwrap_or_else(|| "invalid-port".to_string());
        if is_registered {
            info!(port_name, "JACK port registered");
        } else {
            info!(port_name, "JACK port removed");
        }

        if is_registered && self.0.autoconnect {
            self.0.autoconnect_port(client, &port);
        }
    }

    fn port_rename(
        &mut self,
        client: &jack::Client,
        port_id: jack::PortId,
        old_name: &str,
        new_name: &str,
    ) -> jack::Control {
        info!(
            port_name = log_port(client, port_id),
            old_name, new_name, "JACK port renamed"
        );
        jack::Control::Continue
    }

    fn ports_connected(
        &mut self,
        client: &jack::Client,
        port_id_a: jack::PortId,
        port_id_b: jack::PortId,
        are_connected: bool,
    ) {
        info!(
            port_name_a = log_port(client, port_id_a),
            port_name_b = log_port(client, port_id_b),
            are_connected,
            "JACK ports connected"
        );
    }

    fn graph_reorder(&mut self, _: &jack::Client) -> jack::Control {
        info!("JACK graph reordered");
        jack::Control::Continue
    }

    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        warn!("JACK xrun");
        jack::Control::Continue
    }
}

fn make_ports<PS: Clone + jack::PortSpec>(
    client: &jack::Client,
    spec: PS,
    prefix: &str,
) -> Result<[jack::Port<PS>; 2], jack::Error> {
    Ok([
        client.register_port(&format!("{prefix}-0"), spec.clone())?,
        client.register_port(&format!("{prefix}-1"), spec)?,
    ])
}

fn log_port(client: &jack::Client, port_id: jack::PortId) -> String {
    client
        .port_by_id(port_id)
        .and_then(|port| port.name().ok())
        .unwrap_or_else(|| "unknown".to_string())
}
