use anyhow::Result;
use tracing::{error, info, warn};

pub fn init_jack() -> Result<jack::AsyncClient<PortManager, ()>> {
    let (client, status) = jack::Client::new("tinex", jack::ClientOptions::default())?;
    info!(?client, "Created JACK client");
    if !status.is_empty() {
        warn!(?status, "Got non-empty JACK status.");
    }

    let ports = PortManager::new(&client)?;
    let client = client.activate_async(ports, ())?;
    Ok(client)
}

pub struct PortManager {
    _midi_in: Vec<jack::Port<jack::MidiIn>>,
    _audio_in: Vec<jack::Port<jack::AudioIn>>,
    _audio_out: Vec<jack::Port<jack::AudioOut>>,
}

impl PortManager {
    fn new(client: &jack::Client) -> Result<PortManager> {
        Ok(PortManager {
            _midi_in: make_ports(client, jack::MidiIn::default(), "midi-in", 4)?,
            _audio_in: make_ports(client, jack::AudioIn::default(), "audio-in", 2)?,
            _audio_out: make_ports(client, jack::AudioOut::default(), "audio-out", 2)?,
        })
    }
}

impl jack::NotificationHandler for PortManager {
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
        if client.is_mine(&port) {
            return;
        }
        let port_name = port
            .name()
            .ok()
            .unwrap_or_else(|| "invalid-port".to_string());
        if is_registered {
            info!(port_name, "JACK port registered");
        } else {
            info!(port_name, "JACK port removed");
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
