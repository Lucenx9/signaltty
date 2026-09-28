use std::path::PathBuf;

use clap::Parser;

use signaltty_server::{serve, Config};

#[derive(Debug, Parser)]
#[command(
    name = "signaltty-server",
    about = "signaltty session server (owns PTYs, state, IPC)"
)]
struct Args {
    /// Unix socket path (default: $XDG_RUNTIME_DIR/signaltty/signaltty.sock).
    #[arg(long)]
    socket: Option<PathBuf>,
    /// State directory (default: $XDG_STATE_HOME/signaltty).
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// Plugin directory (default: $XDG_CONFIG_HOME/signaltty/plugins).
    #[arg(long)]
    plugin_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    let mut config = Config::from_env();
    if let Some(s) = args.socket {
        config.socket_path = s;
    }
    if let Some(d) = args.state_dir {
        config.state_dir = d;
    }
    if let Some(d) = args.plugin_dir {
        config.plugin_dir = d;
    }
    serve(config).await
}
