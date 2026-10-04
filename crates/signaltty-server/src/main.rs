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
    /// Agent detection overlays (default: $XDG_CONFIG_HOME/signaltty/agents).
    #[arg(long)]
    agents_dir: Option<PathBuf>,
    /// Maximum parallel running tasks (default: 4).
    #[arg(long = "max-tasks", alias = "max-parallel-tasks")]
    max_tasks: Option<usize>,
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
    if let Some(d) = args.agents_dir {
        config.agents_dir = d;
    }
    if let Some(m) = args.max_tasks {
        config.max_parallel_tasks = m;
    }
    serve(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse_max_tasks() {
        let args = Args::parse_from(["signaltty-server", "--max-tasks", "8"]);
        assert_eq!(args.max_tasks, Some(8));

        let args_alias = Args::parse_from(["signaltty-server", "--max-parallel-tasks", "16"]);
        assert_eq!(args_alias.max_tasks, Some(16));

        let args_default = Args::parse_from(["signaltty-server"]);
        assert_eq!(args_default.max_tasks, None);
    }
}
