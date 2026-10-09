use anyhow::Result;
use clap::Parser;
use tracing::info;

mod gui;
mod jack;
use jack::init_jack;
use tinex_core::{TinexRequest, track::Track};

/// tinex CLI
#[derive(Parser, Debug)]
#[command(name = "tinex", version, about)]
struct Args {
    /// Log level or filter directives (e.g. trace, debug, info, warn, error, `tinex=debug,warn`)
    #[arg(long, default_value = "info")]
    log: String,

    /// True if audio/midi devices should automatically be connected.
    #[arg(long, default_value = "true")]
    autoconnect: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    init_logging(&args);
    let handle = init_jack(args.autoconnect)?;
    let mut track = Track::new();
    track.push_plugin(tinex_plugins::EPiano::new(
        handle.client.as_client().sample_rate() as f32,
    ));
    track.push_plugin(tinex_plugins::Tremolo::new(
        handle.client.as_client().sample_rate() as f32,
    ));
    track.push_plugin(tinex_plugins::Delay::new(
        handle.client.as_client().sample_rate() as f32,
    ));
    handle
        .requests
        .send(TinexRequest::NewTrack(track))
        .map_err(|_| anyhow::anyhow!("Could not request startup PhysicalPiano track"))?;

    gui::run(handle)?;
    tracing::info!("tinex finished");
    Ok(())
}

fn init_logging(args: &Args) {
    let filter = tracing_subscriber::EnvFilter::new(&args.log);

    tracing_subscriber::fmt().with_env_filter(filter).init();
    let cwd = std::env::current_dir().unwrap_or_default();
    info!("Running from {cwd:?} with {args:?}");
}
