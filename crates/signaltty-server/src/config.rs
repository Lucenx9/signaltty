use std::path::PathBuf;

use signaltty_core::paths;

pub const DEFAULT_MAX_PARALLEL_TASKS: usize = 4;
pub const DEFAULT_WORKER_SILENT_TIMEOUT_S: u64 = 600;
pub const DEFAULT_MERGE_TIMEOUT_MS: u64 = 120_000;

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
    /// Deadline for the `task.finish` merge (hooks and signing included).
    pub merge_timeout_ms: u64,
}

impl Config {
    pub fn from_env() -> Config {
        Config::try_from_env().expect("invalid server configuration")
    }

    /// Like `from_env`, but an invalid value (`SIGNALTTY_MAX_PARALLEL_TASKS=0`)
    /// is an error the caller can report instead of a silently useless server.
    pub fn try_from_env() -> Result<Config, String> {
        let max_parallel_tasks = parse_max_tasks(
            std::env::var("SIGNALTTY_MAX_PARALLEL_TASKS")
                .ok()
                .as_deref(),
        )?;
        let worker_silent_timeout_s = parse_worker_silent_timeout(
            std::env::var("SIGNALTTY_WORKER_SILENT_TIMEOUT_S")
                .ok()
                .as_deref(),
        );

        Ok(Config {
            socket_path: paths::socket_path(),
            state_dir: paths::state_dir(),
            history_tail_bytes: 64 * 1024,
            plugin_dir: paths::plugin_dir(),
            agents_dir: paths::agents_dir(),
            integration_home: std::env::var_os("SIGNALTTY_INTEGRATION_HOME").map(PathBuf::from),
            max_parallel_tasks,
            worker_silent_timeout_s,
            merge_timeout_ms: std::env::var("SIGNALTTY_MERGE_TIMEOUT_MS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(DEFAULT_MERGE_TIMEOUT_MS),
        })
    }
}

/// `0` would make every `task.start` fail with `RATE_LIMITED`; it is a
/// configuration error. Unparsable values keep falling back to the default.
pub fn parse_max_tasks(val: Option<&str>) -> Result<usize, String> {
    match val.and_then(|v| v.parse::<usize>().ok()) {
        Some(0) => {
            Err("max parallel tasks must be at least 1 (0 would reject every task.start)".into())
        }
        Some(n) => Ok(n),
        None => Ok(DEFAULT_MAX_PARALLEL_TASKS),
    }
}

/// clap value parser for `--max-tasks`: a positive integer, strictly.
pub fn parse_positive_tasks(val: &str) -> Result<usize, String> {
    match val.parse::<usize>() {
        Ok(0) => Err("must be at least 1 (0 would reject every task.start)".into()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
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
        assert_eq!(parse_max_tasks(None), Ok(DEFAULT_MAX_PARALLEL_TASKS));
        assert_eq!(parse_max_tasks(None), Ok(4));
        assert_eq!(parse_max_tasks(Some("12")), Ok(12));
        assert!(parse_max_tasks(Some("0"))
            .unwrap_err()
            .contains("at least 1"));
        assert_eq!(
            parse_max_tasks(Some("not-a-number")),
            Ok(DEFAULT_MAX_PARALLEL_TASKS)
        );
        assert_eq!(parse_max_tasks(Some("-1")), Ok(DEFAULT_MAX_PARALLEL_TASKS));
        assert!(parse_positive_tasks("0").is_err());
        assert!(parse_positive_tasks("x").is_err());
        assert_eq!(parse_positive_tasks("3"), Ok(3));
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

        std::env::set_var("SIGNALTTY_MAX_PARALLEL_TASKS", "0");
        assert!(Config::try_from_env().unwrap_err().contains("at least 1"));

        std::env::remove_var("SIGNALTTY_MAX_PARALLEL_TASKS");
    }
}
