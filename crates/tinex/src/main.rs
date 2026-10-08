use anyhow::Result;
use clap::Parser;
use std::time::{Duration, Instant};
use tracing::info;

mod jack;
mod tinex;
use jack::init_jack;
use tinex::TinexNotification;

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

    let mut window_start = Instant::now();
    let mut window_peak = 0.0_f32;
    for _ in 0..1000 {
        std::thread::sleep(Duration::from_millis(10));
        for notification in handle.notifications.try_iter() {
            match notification {
                TinexNotification::OutputLevel(output_level) => {
                    window_peak = window_peak.max(output_level);
                }
                notification => info!(?notification, "Tinex notification"),
            }
        }
        if window_start.elapsed() >= Duration::from_secs(1) {
            info!(output_level = window_peak, "Tinex output level");
            window_peak = 0.0;
            window_start = Instant::now();
        }
    }
    tracing::info!("tinex finished");
    Ok(())
}

fn init_logging(args: &Args) {
    let filter = tracing_subscriber::EnvFilter::new(&args.log);

    tracing_subscriber::fmt().with_env_filter(filter).init();
    let cwd = std::env::current_dir().unwrap_or_default();
    info!("Running from {cwd:?} with {args:?}");
}
