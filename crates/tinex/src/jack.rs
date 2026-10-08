use anyhow::Result;
use jack::PortSpec;
use std::sync::Arc;
use tracing::{error, info, warn};

/// The returned port handles are valid while the JACK client remains active.
pub fn init_jack(
    autoconnect: bool,
) -> Result<(
    jack::AsyncClient<SharedNotificationHandler, ProcessHandler>,
    Arc<NotificationHandler>,
)> {
    let (client, status) = jack::Client::new("tinex", jack::ClientOptions::default())?;
    info!(?client, "Created JACK client");
    if !status.is_empty() {
        warn!(?status, "Got non-empty JACK status.");
    }

    let process_handler = ProcessHandler::new(&client)?;
    let notification_handler = Arc::new(NotificationHandler::new(&process_handler, autoconnect)?);
    let client = client.activate_async(
        SharedNotificationHandler(Arc::clone(&notification_handler)),
        process_handler,
    )?;
    Ok((client, notification_handler))
}

pub struct ProcessHandler {
    midi_in: Vec<jack::Port<jack::MidiIn>>,
    audio_in: Vec<jack::Port<jack::AudioIn>>,
    audio_out: Vec<jack::Port<jack::AudioOut>>,
}

impl ProcessHandler {
    fn new(client: &jack::Client) -> Result<ProcessHandler> {
        Ok(ProcessHandler {
            midi_in: make_ports(client, jack::MidiIn::default(), "midi-in", 4)?,
            audio_in: make_ports(client, jack::AudioIn::default(), "audio-in", 2)?,
            audio_out: make_ports(client, jack::AudioOut::default(), "audio-out", 2)?,
        })
    }
}

impl jack::ProcessHandler for ProcessHandler {
    fn process(&mut self, _: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        for out in self.audio_out.iter_mut() {
            let out = out.as_mut_slice(ps);
            out.fill(0.0);
        }
        jack::Control::Continue
    }
}

pub struct NotificationHandler {
    autoconnect: bool,
    pub midi_in: Vec<jack::Port<jack::Unowned>>,
    pub audio_in: Vec<jack::Port<jack::Unowned>>,
    pub audio_out: Vec<jack::Port<jack::Unowned>>,
}

/// Adapts the shared handler to JACK's notification callback trait.
pub struct SharedNotificationHandler(Arc<NotificationHandler>);

impl NotificationHandler {
    fn new(process_handler: &ProcessHandler, autoconnect: bool) -> Result<NotificationHandler> {
        let midi_in = process_handler
            .midi_in
            .iter()
            .map(|port| port.clone_unowned())
            .collect();
        let audio_in = process_handler
            .audio_in
            .iter()
            .map(|port| port.clone_unowned())
            .collect();
        let audio_out = process_handler
            .audio_out
            .iter()
            .map(|port| port.clone_unowned())
            .collect();
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

        if !client.is_mine(port) {
            return;
        }

        let Ok(local_port_name) = port.name() else {
            warn!("Could not determine port name for autoconnect");
            return;
        };
        let Ok(port_type) = port.port_type() else {
            warn!("Could not determine port type for autoconnect");
            return;
        };
        let flags = port.flags();
        let direction = if flags.contains(jack::PortFlags::IS_INPUT) {
            Direction::Input
        } else if flags.contains(jack::PortFlags::IS_OUTPUT) {
            Direction::Output
        } else {
            return;
        };
        let local_ports = match direction {
            Direction::Input => {
                if port_type == jack::MidiIn::default().jack_port_type() {
                    &self.midi_in
                } else if port_type == jack::AudioIn::default().jack_port_type() {
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
    count: usize,
) -> Result<Vec<jack::Port<PS>>, jack::Error> {
    (0..count)
        .map(|index| {
            let name = format!("{prefix}-{index}");
            client.register_port(&name, spec.clone())
        })
        .collect()
}

fn log_port(client: &jack::Client, port_id: jack::PortId) -> String {
    client
        .port_by_id(port_id)
        .and_then(|port| port.name().ok())
        .unwrap_or_else(|| "unknown".to_string())
}
