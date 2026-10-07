use anyhow::Result;
use clap::Parser;
use tracing::info;

mod jack;
use jack::init_jack;

/// tinex CLI
#[derive(Parser, Debug)]
#[command(name = "tinex", version, about)]
struct Args {
    /// Log level or filter directives (e.g. trace, debug, info, warn, error, `tinex=debug,warn`)
    #[arg(long, default_value = "info")]
    log: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    init_logging(&args);
    let _client = init_jack()?;

    std::thread::sleep(std::time::Duration::from_secs(10));
    tracing::info!("tinex finished");
    Ok(())
}

fn init_logging(args: &Args) {
    let filter = tracing_subscriber::EnvFilter::new(&args.log);

    tracing_subscriber::fmt().with_env_filter(filter).init();
    let cwd = std::env::current_dir().unwrap_or_default();
    info!("Running from {cwd:?} with {args:?}");
}
