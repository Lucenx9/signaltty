use std::path::PathBuf;

use signaltty_core::paths;

pub const DEFAULT_MAX_PARALLEL_TASKS: usize = 4;
pub const DEFAULT_WORKER_SILENT_TIMEOUT_S: u64 = 600;

#[derive(Debug, Clone)]
pub struct Config {
    pub socket_path: PathBuf,
    pub state_dir: PathBuf,
    pub history_tail_bytes: usize,
    pub plugin_dir: PathBuf,
    pub agents_dir: PathBuf,
    pub integration_home: Option<PathBuf>,
    pub max_parallel_tasks: usize,
    pub worker_silent_timeout_s: u64,
}

impl Config {
    pub fn from_env() -> Config {
        let max_parallel_tasks = parse_max_tasks(
            std::env::var("SIGNALTTY_MAX_PARALLEL_TASKS")
                .ok()
                .as_deref(),
        );
        let worker_silent_timeout_s = parse_worker_silent_timeout(
            std::env::var("SIGNALTTY_WORKER_SILENT_TIMEOUT_S")
                .ok()
                .as_deref(),
        );

        Config {
            socket_path: paths::socket_path(),
            state_dir: paths::state_dir(),
            history_tail_bytes: 64 * 1024,
            plugin_dir: paths::plugin_dir(),
            agents_dir: paths::agents_dir(),
            integration_home: std::env::var_os("SIGNALTTY_INTEGRATION_HOME").map(PathBuf::from),
            max_parallel_tasks,
            worker_silent_timeout_s,
        }
    }
}

pub fn parse_max_tasks(val: Option<&str>) -> usize {
    val.and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_PARALLEL_TASKS)
}

pub fn parse_worker_silent_timeout(val: Option<&str>) -> u64 {
    val.and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_WORKER_SILENT_TIMEOUT_S)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_max_tasks_logic() {
        assert_eq!(parse_max_tasks(None), DEFAULT_MAX_PARALLEL_TASKS);
        assert_eq!(parse_max_tasks(None), 4);
        assert_eq!(parse_max_tasks(Some("12")), 12);
        assert_eq!(parse_max_tasks(Some("0")), 0);
        assert_eq!(
            parse_max_tasks(Some("not-a-number")),
            DEFAULT_MAX_PARALLEL_TASKS
        );
        assert_eq!(parse_max_tasks(Some("-1")), DEFAULT_MAX_PARALLEL_TASKS);
    }

    #[test]
    fn config_from_env_sequential() {
        let _guard = ENV_MUTEX.lock().unwrap();

        std::env::remove_var("SIGNALTTY_MAX_PARALLEL_TASKS");
        let cfg = Config::from_env();
        assert_eq!(cfg.max_parallel_tasks, DEFAULT_MAX_PARALLEL_TASKS);

        std::env::set_var("SIGNALTTY_MAX_PARALLEL_TASKS", "16");
        let cfg2 = Config::from_env();
        assert_eq!(cfg2.max_parallel_tasks, 16);

        std::env::set_var("SIGNALTTY_MAX_PARALLEL_TASKS", "invalid");
        let cfg3 = Config::from_env();
        assert_eq!(cfg3.max_parallel_tasks, DEFAULT_MAX_PARALLEL_TASKS);

        std::env::remove_var("SIGNALTTY_MAX_PARALLEL_TASKS");
    }
}
