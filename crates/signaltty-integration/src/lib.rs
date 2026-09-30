//! OS-facing hook configuration shared by CLI and server (ADR-0013).
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::fcntl::{Flock, FlockArg};
use serde::Serialize;
use serde_json::{json, Value};

pub const MARKER: &str = "signaltty hook-event";
const MANAGED: &str = " # signaltty hook-event (managed)";
pub const AGENTS: &[&str] = &["codex", "claude", "opencode", "cursor"];
fn hook_events(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &[
            "SessionStart",
            "UserPromptSubmit",
            "Notification",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ],
        "codex" => &[
            "SessionStart",
            "UserPromptSubmit",
            "PermissionRequest",
            "Stop",
            "SessionEnd",
        ],
        "cursor" => &["sessionStart", "beforeSubmitPrompt", "stop", "sessionEnd"],
        _ => &[],
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub agent: String,
    pub file: PathBuf,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

#[derive(Clone)]
pub struct Hooks {
    home: PathBuf,
    reporter: PathBuf,
    claude_home: PathBuf,
    codex_home: PathBuf,
    opencode_home: PathBuf,
}

impl Hooks {
    /// Explicit roots are useful for isolated installation and tests.
    pub fn new(home: PathBuf, config_home: PathBuf, reporter: PathBuf) -> Self {
        Self {
            claude_home: home.join(".claude"),
            codex_home: home.join(".codex"),
            opencode_home: config_home.join("opencode"),
            home,
            reporter,
        }
    }

    /// An explicit home isolates provider roots; otherwise honor provider overrides.
    pub fn from_env(home: Option<&Path>, reporter: PathBuf) -> Result<Self, Error> {
        let root = home
            .map(Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .ok_or_else(|| Error("HOME is not set".into()))?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join(".config"));
        let mut hooks = Self::new(root, config, reporter);
        if home.is_none() {
            if let Some(p) = std::env::var_os("CLAUDE_CONFIG_DIR") {
                hooks.claude_home = p.into();
            }
            if let Some(p) = std::env::var_os("CODEX_HOME") {
                hooks.codex_home = p.into();
            }
            if let Some(p) = std::env::var_os("OPENCODE_CONFIG_DIR") {
                hooks.opencode_home = p.into();
            }
        }
        Ok(hooks)
    }

    pub fn with_overrides(
        mut self,
        env: &std::collections::HashMap<String, String>,
        cwd: &Path,
    ) -> Self {
        if let Some(p) = env.get("CLAUDE_CONFIG_DIR") {
            self.claude_home = p.into();
        }
        if let Some(p) = env.get("CODEX_HOME") {
            self.codex_home = p.into();
        }
        if let Some(p) = env.get("OPENCODE_CONFIG_DIR") {
            self.opencode_home = p.into();
        }
        for root in [
            &mut self.home,
            &mut self.claude_home,
            &mut self.codex_home,
            &mut self.opencode_home,
        ] {
            if root.is_relative() {
                *root = cwd.join(&*root);
            }
        }
        self
    }

    pub fn file_for(&self, agent: &str) -> Result<PathBuf, Error> {
        Ok(match agent {
            "claude" => self.claude_home.join("settings.json"),
            "codex" => self.codex_home.join("hooks.json"),
            "cursor" => self.home.join(".cursor/hooks.json"),
            "opencode" => self.opencode_home.join("plugins/signaltty.js"),
            _ => return Err(Error(format!("unknown agent '{agent}'"))),
        })
    }

    pub fn install(&self, agent: &str) -> Result<Report, Error> {
        if !is_executable(&self.reporter) {
            return Err(Error(format!(
                "signaltty hook reporter is unavailable: {}",
                self.reporter.display()
            )));
        }
        self.update(agent, false)
    }

    pub fn uninstall(&self, agent: &str) -> Result<Report, Error> {
        self.update(agent, true)
    }

    pub fn installed(&self, agent: &str) -> Result<bool, Error> {
        let path = self.file_for(agent)?;
        let Some(text) = read(&path)? else {
            return Ok(false);
        };
        if agent == "opencode" {
            return Ok(text.starts_with(PLUGIN_HEADER));
        }
        let root = parse(&path, &text)?;
        Ok(root
            .get("hooks")
            .and_then(Value::as_object)
            .is_some_and(|map| {
                map.values().any(|v| {
                    v.as_array().is_some_and(|arr| {
                        arr.iter().any(|group| {
                            owned(group)
                                || group
                                    .get("hooks")
                                    .and_then(Value::as_array)
                                    .is_some_and(|hooks| hooks.iter().any(owned))
                        })
                    })
                })
            }))
    }

    fn update(&self, agent: &str, remove: bool) -> Result<Report, Error> {
        let destination = self.file_for(agent)?;
        if remove && !destination.try_exists()? {
            return Ok(report(agent, destination, false, false));
        }
        // Resolve existing symlinks, preserving the link and target permissions.
        let path = match fs::symlink_metadata(&destination) {
            Ok(m) if m.file_type().is_symlink() => fs::canonicalize(&destination)?,
            Ok(_) | Err(_) => destination.clone(),
        };
        let parent = path
            .parent()
            .ok_or_else(|| Error("configuration has no parent".into()))?;
        fs::create_dir_all(parent)?;
        let lock_path = path.with_file_name(format!(
            ".{}.signaltty.lock",
            path.file_name().unwrap().to_string_lossy()
        ));
        let _lock = lock(&lock_path)?;
        let before = read(&path)?;
        let after = if agent == "opencode" {
            if before
                .as_ref()
                .is_some_and(|s| !s.starts_with(PLUGIN_HEADER))
            {
                if remove {
                    return Ok(report(agent, destination, false, false));
                }
                return Err(Error(format!(
                    "{} already contains a plugin not managed by Signaltty",
                    destination.display()
                )));
            }
            if remove {
                None
            } else {
                Some(OPENCODE_PLUGIN.replace(
                    "__SIGNALTTY_CLI__",
                    &serde_json::to_string(&self.reporter.to_string_lossy()).unwrap(),
                ))
            }
        } else {
            let mut root = match &before {
                Some(s) => parse(&path, s)?,
                None => json!({}),
            };
            let original = root.clone();
            if remove {
                prune(&mut root, agent == "cursor")?;
            } else {
                self.merge(&mut root, agent)?;
            }
            if root == original {
                before.clone()
            } else {
                Some(serde_json::to_string_pretty(&root).unwrap() + "\n")
            }
        };
        let changed = before != after;
        if changed {
            // Do not knowingly clobber an external writer that ignored our advisory lock.
            if read(&path)? != before {
                return Err(Error(format!(
                    "{} changed during setup; retry",
                    destination.display()
                )));
            }
            if let Some(text) = after {
                atomic_write(&path, text.as_bytes())?;
            } else if before.is_some() {
                fs::remove_file(&path)?;
            }
        }
        Ok(report(agent, destination, changed, !remove))
    }

    fn merge(&self, root: &mut Value, agent: &str) -> Result<(), Error> {
        if agent == "cursor" && root.get("version").is_none() {
            root["version"] = json!(1);
        }
        let hooks = root
            .as_object_mut()
            .unwrap()
            .entry("hooks")
            .or_insert_with(|| json!({}));
        let map = hooks
            .as_object_mut()
            .ok_or_else(|| Error("'hooks' must be an object".into()))?;
        let events = hook_events(agent);
        for event in events {
            let waiting = matches!(agent, "claude" | "codex") && *event == "PermissionRequest";
            let timeout = if waiting {
                125
            } else if agent == "codex" {
                3
            } else {
                10
            };
            let wait_flag = if waiting { " --wait-for-answer" } else { "" };
            let list = map.entry(*event).or_insert_with(|| json!([]));
            let arr = list
                .as_array_mut()
                .ok_or_else(|| Error(format!("hooks.{event} must be an array")))?;
            let desired = format!(
                "{} hook-event --agent {agent} --event {event} --payload-stdin{wait_flag}{MANAGED}",
                shell_quote(&self.reporter.to_string_lossy())
            );
            let mut found = false;
            for group in arr.iter_mut() {
                if agent == "cursor" {
                    if owned(group) {
                        group["command"] = json!(desired);
                        group["timeout"] = json!(timeout);
                        found = true;
                    }
                } else if let Some(commands) = group.get_mut("hooks").and_then(Value::as_array_mut)
                {
                    for command in commands.iter_mut().filter(|c| owned(c)) {
                        command["command"] = json!(desired);
                        command["timeout"] = json!(timeout);
                        found = true;
                    }
                }
            }
            if !found {
                arr.push(if agent == "cursor" { json!({"command": desired, "timeout": timeout}) }
                    else { json!({"matcher": "", "hooks": [{"type": "command", "command": desired, "timeout": timeout}]}) });
            }
        }
        Ok(())
    }
}

fn report(agent: &str, file: PathBuf, changed: bool, installing: bool) -> Report {
    Report { agent: agent.into(), file, changed,
        notice: (agent == "codex" && installing && changed).then(|| "Codex: review and trust the Signaltty hooks in /hooks before status events can arrive. In an ordinary shell, launch codex --no-daemon so hooks belong to this terminal.".into()) }
}

fn parse(path: &Path, text: &str) -> Result<Value, Error> {
    let root: Value = serde_json::from_str(text)
        .map_err(|e| Error(format!("{}: invalid JSON: {e}", path.display())))?;
    if !root.is_object() {
        return Err(Error(format!(
            "{}: configuration must be an object",
            path.display()
        )));
    }
    Ok(root)
}

fn read(path: &Path) -> Result<Option<String>, Error> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !file.metadata()?.is_file() {
        return Err(Error(format!(
            "{}: configuration must be a regular file",
            path.display()
        )));
    }
    const LIMIT: u64 = 4 * 1024 * 1024;
    let mut text = String::new();
    file.take(LIMIT + 1).read_to_string(&mut text)?;
    if text.len() as u64 > LIMIT {
        return Err(Error(format!(
            "{}: configuration exceeds 4 MiB",
            path.display()
        )));
    }
    Ok(Some(text))
}

fn owned(value: &Value) -> bool {
    let Some(cmd) = value.get("command").and_then(Value::as_str) else {
        return false;
    };
    if cmd.ends_with(MANAGED) {
        return true;
    }
    // Legacy generated command: exact shape, never arbitrary strings mentioning the marker.
    let Some((binary, args)) = cmd.split_once(" hook-event --agent ") else {
        return false;
    };
    let parts: Vec<_> = args.split_whitespace().collect();
    (binary == "signaltty" || binary.starts_with('/'))
        && binary
            .chars()
            .all(|c| c.is_alphanumeric() || "/._-".contains(c))
        && Path::new(binary)
            .file_name()
            .is_some_and(|p| p == "signaltty")
        && parts.len() == 4
        && AGENTS.contains(&parts[0])
        && parts[1] == "--event"
        && parts[3] == "--payload-stdin"
        && hook_events(parts[0]).contains(&parts[2])
        && args == format!("{} --event {} --payload-stdin", parts[0], parts[2])
}

fn prune(root: &mut Value, cursor: bool) -> Result<(), Error> {
    let Some(hooks) = root.get_mut("hooks") else {
        return Ok(());
    };
    let map = hooks
        .as_object_mut()
        .ok_or_else(|| Error("'hooks' must be an object".into()))?;
    let mut emptied_events = Vec::new();
    for (event, list) in map.iter_mut() {
        let arr = list
            .as_array_mut()
            .ok_or_else(|| Error("hook events must be arrays".into()))?;
        let originally_empty = arr.is_empty();
        arr.retain_mut(|group| {
            if cursor {
                return !owned(group);
            }
            let only_managed_fields = group.as_object().is_some_and(|object| {
                object.keys().all(|key| key == "matcher" || key == "hooks")
                    && object.get("matcher").is_none_or(|v| v == "")
            });
            if let Some(commands) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                let before = commands.len();
                commands.retain(|c| !owned(c));
                if before != commands.len() && commands.is_empty() && only_managed_fields {
                    return false;
                }
            }
            true
        });
        if !originally_empty && arr.is_empty() {
            emptied_events.push(event.clone());
        }
    }
    for event in emptied_events {
        map.remove(&event);
    }
    Ok(())
}

fn lock(path: &Path) -> Result<Flock<File>, Error> {
    let started = Instant::now();
    loop {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_NONBLOCK | nix::libc::O_NOFOLLOW)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error("configuration lock must be a regular file".into()));
        }
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(lock) => return Ok(lock),
            Err((_, nix::errno::Errno::EWOULDBLOCK))
                if started.elapsed() < Duration::from_millis(100) =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err((_, e)) => return Err(Error(format!("configuration lock unavailable: {e}"))),
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_file_name(format!(
        ".{}.signaltty-{}-{}",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), Error> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        if let Ok(meta) = fs::metadata(path) {
            file.set_permissions(meta.permissions())?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

pub fn is_executable(path: &Path) -> bool {
    path.is_absolute()
        && fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub fn reporter_executable() -> Result<PathBuf, Error> {
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("signaltty");
        if is_executable(&sibling) {
            return Ok(sibling);
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = dir.join("signaltty");
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(Error(
        "signaltty CLI is missing; install it beside signaltty-server or on PATH".into(),
    ))
}

const PLUGIN_HEADER: &str =
    "// signaltty hook-event shim for OpenCode (managed by `signaltty integration`).";

const OPENCODE_PLUGIN: &str = r#"// signaltty hook-event shim for OpenCode (managed by `signaltty integration`).
// Reports session lifecycle to the signaltty server via `hook-event`.
// Safe no-op outside signaltty panes (no SIGNALTTY_PANE → accepted, ignored).
import { spawnSync } from "node:child_process";

const CLI = __SIGNALTTY_CLI__;

function report(hook, payload) {
  if (!process.env.SIGNALTTY_PANE) return;
  try {
    spawnSync(CLI, ["hook-event", "--agent", "opencode", "--event", hook, "--payload-stdin"], {
      input: JSON.stringify(payload ?? {}),
      timeout: 8000,
      stdio: ["pipe", "ignore", "ignore"],
    });
  } catch {
    // never break the agent session
  }
}

function sessionId(event) {
  const p = event.properties ?? {};
  return p.sessionID ?? p.sessionId ?? p.session_id ?? event.sessionID ?? null;
}

const SignalttyPlugin = async () => ({
  event: async ({ event }) => {
    const sid = sessionId(event);
    switch (event.type) {
      case "session.created":
        if (sid) report("session.created", { session_id: sid });
        break;
      case "session.status": {
        const rawStatus = event.properties?.status;
        const status = typeof rawStatus === "string" ? rawStatus : rawStatus?.type;
        if (sid && (status === "busy" || status === "idle")) {
          report("session.status", { session_id: sid, status });
        }
        break;
      }
      case "session.idle":
        if (sid) report("session.idle", { session_id: sid });
        break;
      case "session.error":
        report("session.error", {
          ...(sid ? { session_id: sid } : {}),
          message: String(event.properties?.message ?? event.error ?? "unknown"),
        });
        break;
      default:
        break;
    }
  },
});

export default SignalttyPlugin;
export { SignalttyPlugin };
"#;
