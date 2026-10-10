//! XDG paths. Socket lives under $XDG_RUNTIME_DIR (0700 dir, 0600
//! socket); snapshots under $XDG_STATE_HOME.

use std::path::PathBuf;

/// An XDG variable's value; set-but-empty counts as unset (XDG Base
/// Directory spec), so `XDG_STATE_HOME=` never yields a relative path.
fn env_dir(name: &str) -> Option<String> {
    non_empty(std::env::var(name).ok())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

pub fn runtime_dir() -> PathBuf {
    if let Some(dir) = env_dir("XDG_RUNTIME_DIR") {
        PathBuf::from(dir).join("signaltty")
    } else {
        // Fallback for environments without XDG_RUNTIME_DIR.
        PathBuf::from(format!("/tmp/signaltty-{}", current_uid()))
    }
}

fn current_uid() -> u32 {
    if let Ok(uid) = std::env::var("UID") {
        if let Ok(n) = uid.parse() {
            return n;
        }
    }
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn socket_path() -> PathBuf {
    socket_path_from(std::env::var("SIGNALTTY_SOCKET").ok(), runtime_dir())
}

fn socket_path_from(socket: Option<String>, runtime: PathBuf) -> PathBuf {
    socket
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime.join("signaltty.sock"))
}

pub fn state_dir() -> PathBuf {
    if let Some(dir) = env_dir("XDG_STATE_HOME") {
        PathBuf::from(dir).join("signaltty")
    } else if let Some(home) = env_dir("HOME") {
        PathBuf::from(home).join(".local/state/signaltty")
    } else {
        PathBuf::from("/tmp/signaltty-state")
    }
}

pub fn snapshot_path() -> PathBuf {
    state_dir().join("snapshot.json")
}

pub fn history_dir() -> PathBuf {
    state_dir().join("history")
}

pub fn config_dir() -> PathBuf {
    if let Some(dir) = env_dir("XDG_CONFIG_HOME") {
        PathBuf::from(dir).join("signaltty")
    } else if let Some(home) = env_dir("HOME") {
        PathBuf::from(home).join(".config/signaltty")
    } else {
        PathBuf::from("/tmp/signaltty-config")
    }
}

pub fn data_dir() -> PathBuf {
    data_dir_from(
        std::env::var("XDG_DATA_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

fn data_dir_from(xdg: Option<String>, home: Option<String>) -> PathBuf {
    if let Some(dir) = non_empty(xdg) {
        PathBuf::from(dir).join("signaltty")
    } else if let Some(home) = non_empty(home) {
        PathBuf::from(home).join(".local/share/signaltty")
    } else {
        PathBuf::from("/tmp/signaltty-data")
    }
}

/// Program for a pane launched without argv: `$SHELL`, or `sh` when it is
/// unset or empty (an empty program name cannot be spawned).
pub fn user_shell() -> String {
    shell_from(std::env::var("SHELL").ok())
}

fn shell_from(shell: Option<String>) -> String {
    non_empty(shell).unwrap_or_else(|| "sh".to_string())
}

/// Directory scanned for plugin packages (`<name>/plugin.toml`).
pub fn plugin_dir() -> PathBuf {
    std::env::var("SIGNALTTY_PLUGIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir().join("plugins"))
}

/// Directory scanned for agent detection overlays (`<name>.toml`).
/// See docs/07, ADR-0009.
pub fn agents_dir() -> PathBuf {
    std::env::var("SIGNALTTY_AGENTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| config_dir().join("agents"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Resolvers take the values as parameters: tests never touch the shared
    // process environment (set_var races other threads' getenv).
    #[test]
    fn data_dir_xdg_env_override_wins() {
        assert_eq!(
            data_dir_from(Some("/tmp/custom-xdg-data".into()), Some("/home/u".into())),
            PathBuf::from("/tmp/custom-xdg-data/signaltty")
        );
        assert_eq!(
            data_dir_from(None, Some("/home/u".into())),
            PathBuf::from("/home/u/.local/share/signaltty")
        );
        assert_eq!(
            data_dir_from(None, None),
            PathBuf::from("/tmp/signaltty-data")
        );
    }

    #[test]
    fn empty_xdg_values_count_as_unset() {
        // `XDG_DATA_HOME=` used to resolve to the relative `signaltty`.
        assert_eq!(
            data_dir_from(Some(String::new()), Some("/home/u".into())),
            PathBuf::from("/home/u/.local/share/signaltty")
        );
        assert_eq!(
            data_dir_from(Some(String::new()), Some(String::new())),
            PathBuf::from("/tmp/signaltty-data")
        );
    }

    #[test]
    fn empty_shell_counts_as_unset() {
        // `SHELL=` used to launch an empty program name, which fails to spawn.
        assert_eq!(shell_from(Some("/bin/zsh".into())), "/bin/zsh");
        assert_eq!(shell_from(Some(String::new())), "sh");
        assert_eq!(shell_from(None), "sh");
    }

    #[test]
    fn socket_env_override_wins() {
        assert_eq!(
            socket_path_from(Some("/tmp/test-signaltty.sock".into()), "/run/x".into()),
            PathBuf::from("/tmp/test-signaltty.sock")
        );
        assert_eq!(
            socket_path_from(None, "/run/x".into()),
            PathBuf::from("/run/x/signaltty.sock")
        );
    }
}
