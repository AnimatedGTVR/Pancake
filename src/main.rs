use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod backend;
mod config;
mod handlers;
mod render;
mod shell;
mod state;
mod syrup;

/// Pancake — the Sweetest, Smoothest Desktop Environment
#[derive(Debug, Parser)]
#[command(name = "pancake", version, about)]
struct Args {
    /// Run inside an existing compositor using the Winit backend (for development)
    #[arg(long, default_value_t = false)]
    winit: bool,

    /// Force a specific TTY number (e.g. --tty 1). Defaults to current TTY.
    #[arg(long)]
    tty: Option<u8>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pancake=debug,smithay=warn".parse().unwrap()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    config::install_sighup_handler();

    if args.winit {
        info!("Starting Pancake with Winit backend (development mode)");
        backend::winit::run()
    } else {
        info!("Starting Pancake with udev/DRM backend");
        backend::udev::run(args.tty)
    }
}
