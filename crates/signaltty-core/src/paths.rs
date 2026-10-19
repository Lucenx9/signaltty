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
    path_override(socket).unwrap_or_else(|| runtime.join("signaltty.sock"))
}

/// A `SIGNALTTY_*` path override; set-but-empty counts as unset, like the
/// XDG variables, so `SIGNALTTY_SOCKET=` never resolves to an empty path.
fn path_override(value: Option<String>) -> Option<PathBuf> {
    non_empty(value).map(PathBuf::from)
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
    plugin_dir_from(
        std::env::var("SIGNALTTY_PLUGIN_DIR").ok(),
        std::env::var("SIGNALTTY_PLUGIN_NAME").ok(),
        config_dir().join("plugins"),
    )
}

/// Hooks and `plugin run` commands export `SIGNALTTY_PLUGIN_DIR` as their
/// own package dir (`<root>/<name>`, enforced at load) next to
/// `SIGNALTTY_PLUGIN_NAME`. Inside one, the root is that dir's parent, so a
/// nested `signaltty plugin run` (or a server it starts) finds every plugin.
fn plugin_dir_from(dir: Option<String>, name: Option<String>, default: PathBuf) -> PathBuf {
    let Some(dir) = path_override(dir) else {
        return default;
    };
    // Resolve a relative override now: plugin commands and hooks run from
    // their package dir, so a relative export would no longer point here.
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    match (non_empty(name), dir.parent()) {
        (Some(name), Some(root)) if dir.file_name() == Some(name.as_ref()) => root.to_path_buf(),
        _ => dir,
    }
}

/// Directory scanned for agent detection overlays (`<name>.toml`).
/// See docs/07, ADR-0009.
pub fn agents_dir() -> PathBuf {
    path_override(std::env::var("SIGNALTTY_AGENTS_DIR").ok())
        .unwrap_or_else(|| config_dir().join("agents"))
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

    #[test]
    fn plugin_dir_inside_a_plugin_is_the_plugins_root() {
        // Hooks and `plugin run` commands get SIGNALTTY_PLUGIN_DIR set to
        // their own package dir; a nested `signaltty plugin run` must still
        // scan the root, not `<root>/<name>/*/plugin.toml`.
        let default = || PathBuf::from("/home/u/.config/signaltty/plugins");
        assert_eq!(
            plugin_dir_from(
                Some("/srv/plugins/chain".into()),
                Some("chain".into()),
                default()
            ),
            PathBuf::from("/srv/plugins")
        );
        assert_eq!(
            plugin_dir_from(Some("/srv/plugins".into()), None, default()),
            PathBuf::from("/srv/plugins")
        );
        // A name that is not the dir's own leaves the override alone.
        assert_eq!(
            plugin_dir_from(Some("/srv/plugins".into()), Some("chain".into()), default()),
            PathBuf::from("/srv/plugins")
        );
        assert_eq!(
            plugin_dir_from(None, Some("chain".into()), default()),
            default()
        );
        assert_eq!(
            plugin_dir_from(Some(String::new()), Some("chain".into()), default()),
            default()
        );
    }

    #[test]
    fn empty_path_overrides_count_as_unset() {
        // `SIGNALTTY_SOCKET=` used to resolve to an empty socket path.
        assert_eq!(
            socket_path_from(Some(String::new()), "/run/x".into()),
            PathBuf::from("/run/x/signaltty.sock")
        );
        assert_eq!(path_override(Some(String::new())), None);
        assert_eq!(
            path_override(Some("/srv/agents".into())),
            Some(PathBuf::from("/srv/agents"))
        );
    }
}
