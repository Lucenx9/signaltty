use std::path::PathBuf;

use signaltty_core::paths;

#[derive(Debug, Clone)]
pub struct Config {
    pub socket_path: PathBuf,
    pub state_dir: PathBuf,
    pub history_tail_bytes: usize,
    pub plugin_dir: PathBuf,
    pub agents_dir: PathBuf,
    pub integration_home: Option<PathBuf>,
}

impl Config {
    pub fn from_env() -> Config {
        Config {
            socket_path: paths::socket_path(),
            state_dir: paths::state_dir(),
            history_tail_bytes: 64 * 1024,
            plugin_dir: paths::plugin_dir(),
            agents_dir: paths::agents_dir(),
            integration_home: std::env::var_os("SIGNALTTY_INTEGRATION_HOME").map(PathBuf::from),
        }
    }
}
