//! `integration install/uninstall/status`: non-destructive hook shims
//! for codex/claude/opencode/cursor. All shims shell out to
//! `signaltty hook-event`, which resolves the pane from $SIGNALTTY_PANE
//! (set by the server at spawn). Entries are marked so uninstall only
//! removes what install added; user config is otherwise untouched.
//!
//! Executable shims run with the user's privileges and must be trusted;
//! install is always explicit. See docs/10, docs/19.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::client::CliError;

pub const MARKER: &str = "signaltty hook-event";

pub fn home_dir(override_home: Option<&str>) -> Result<PathBuf, CliError> {
    if let Some(h) = override_home {
        return Ok(PathBuf::from(h));
    }
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| CliError::Usage("HOME is not set (use --home)".to_string()))
}

fn cli_path() -> String {
    // Absolute path: hook hosts must not depend on the user's PATH.
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "signaltty".to_string())
}

fn shim_cmd(agent: &str, event: &str) -> String {
    format!(
        "{} hook-event --agent {agent} --event {event} --payload-stdin",
        cli_path()
    )
}

fn read_json(path: &Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

fn write_json(path: &Path, v: &Value) -> Result<(), CliError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| CliError::Io(e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(v).unwrap();
    std::fs::write(path, text + "\n").map_err(|e| CliError::Io(e.to_string()))
}

fn contains_marker(v: &Value) -> bool {
    match v {
        Value::String(s) => s.contains(MARKER),
        Value::Array(a) => a.iter().any(contains_marker),
        Value::Object(o) => o.values().any(contains_marker),
        _ => false,
    }
}

// ---- Claude-style schema (settings.json + codex hooks.json) ----
// {"hooks": {"Stop": [{"matcher": "", "hooks": [{"type": "command",
//   "command": "...", "timeout": 10}]}]}}

const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Notification",
    "Stop",
    "SessionEnd",
];

const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PermissionRequest",
    "Stop",
    "SessionEnd",
];

fn install_claude_style(
    path: &Path,
    agent: &str,
    events: &[&str],
    timeout: u64,
) -> Result<bool, CliError> {
    let mut root = read_json(path);
    if !root.is_object() {
        root = serde_json::json!({});
    }
    let hooks = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));
    let map = hooks
        .as_object_mut()
        .ok_or_else(|| CliError::Usage(format!("{}: 'hooks' is not an object", path.display())))?;
    let mut changed = false;
    for event in events {
        let list = map.entry(*event).or_insert_with(|| serde_json::json!([]));
        let arr = list.as_array_mut().ok_or_else(|| {
            CliError::Usage(format!("{}: hooks.{event} is not an array", path.display()))
        })?;
        let desired = shim_cmd(agent, event);
        let mut has_marker = false;
        for entry in arr.iter_mut() {
            let (found, modified) = refresh_claude_entry(entry, &desired, timeout);
            has_marker |= found;
            changed |= modified;
        }
        if has_marker {
            continue;
        }
        arr.push(serde_json::json!({
            "matcher": "",
            "hooks": [{ "type": "command", "command": desired, "timeout": timeout }]
        }));
        changed = true;
    }
    if changed {
        write_json(path, &root)?;
    }
    Ok(changed)
}

fn uninstall_claude_style(path: &Path) -> Result<bool, CliError> {
    let mut root = read_json(path);
    let Some(map) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return Ok(false);
    };
    let mut changed = false;
    let mut empty_events = Vec::new();
    for (event, list) in map.iter_mut() {
        if let Some(arr) = list.as_array_mut() {
            let before = arr.len();
            arr.retain(|entry| !contains_marker(entry));
            if arr.len() != before {
                changed = true;
            }
            if arr.is_empty() {
                empty_events.push(event.clone());
            }
        }
    }
    for event in empty_events {
        map.remove(&event);
    }
    if changed {
        write_json(path, &root)?;
    }
    Ok(changed)
}

/// Refresh our entries in place (path/timeout drift).
/// Returns (marker found, value modified).
fn refresh_claude_entry(entry: &mut Value, desired: &str, timeout: u64) -> (bool, bool) {
    let mut found = false;
    let mut modified = false;
    if let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
        for hook in hooks.iter_mut() {
            let cmd = hook.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if cmd.contains(MARKER) {
                found = true;
                if cmd != desired {
                    hook["command"] = Value::String(desired.to_string());
                    modified = true;
                }
                if hook.get("timeout").and_then(|t| t.as_u64()) != Some(timeout) {
                    hook["timeout"] = serde_json::json!(timeout);
                    modified = true;
                }
            }
        }
    }
    (found, modified)
}

// ---- Cursor schema: {"version": 1, "hooks": {"stop": [{"command": "..."}]}}

const CURSOR_EVENTS: &[&str] = &["sessionStart", "beforeSubmitPrompt", "stop", "sessionEnd"];

fn install_cursor(path: &Path) -> Result<bool, CliError> {
    let mut root = read_json(path);
    if !root.is_object() {
        root = serde_json::json!({"version": 1});
    }
    if root.get("version").is_none() {
        root["version"] = serde_json::json!(1);
    }
    if root.get("hooks").is_none() {
        root["hooks"] = serde_json::json!({});
    }
    let map = root
        .get_mut("hooks")
        .and_then(|h| h.as_object_mut())
        .ok_or_else(|| CliError::Usage(format!("{}: 'hooks' is not an object", path.display())))?;
    let mut changed = false;
    for event in CURSOR_EVENTS {
        let list = map.entry(*event).or_insert_with(|| serde_json::json!([]));
        let arr = list.as_array_mut().ok_or_else(|| {
            CliError::Usage(format!("{}: hooks.{event} is not an array", path.display()))
        })?;
        let desired = shim_cmd("cursor", event);
        let mut has_current = false;
        for entry in arr.iter_mut() {
            let cmd = entry.get("command").and_then(|c| c.as_str()).unwrap_or("");
            if cmd.contains(MARKER) {
                entry["command"] = Value::String(desired.clone());
                entry["timeout"] = serde_json::json!(10);
                has_current = true;
            }
        }
        if has_current {
            continue;
        }
        arr.push(serde_json::json!({"command": desired, "timeout": 10}));
        changed = true;
    }
    if changed {
        write_json(path, &root)?;
    }
    Ok(changed)
}

fn uninstall_cursor(path: &Path) -> Result<bool, CliError> {
    uninstall_claude_style(path) // same prune logic works (marker-based)
}

// ---- OpenCode plugin (whole file is ours) ----

const OPENCODE_PLUGIN: &str = r#"// signaltty hook-event shim for OpenCode (managed by `signaltty integration`).
// Reports session lifecycle to the signaltty server via `hook-event`.
// Safe no-op outside signaltty panes (no SIGNALTTY_PANE → accepted, ignored).
import { spawnSync } from "node:child_process";

const CLI = "__SIGNALTTY_CLI__";

function report(hook, payload) {
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
        const status = event.properties?.status;
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

fn opencode_plugin_path(home: &Path) -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config"));
    base.join("opencode/plugins/signaltty.js")
}

// ---- public ops ----

pub fn valid_agents() -> &'static [&'static str] {
    &["codex", "claude", "opencode", "cursor"]
}

fn file_for(home: &Path, agent: &str) -> PathBuf {
    match agent {
        "claude" => home.join(".claude/settings.json"),
        "codex" => home.join(".codex/hooks.json"),
        "cursor" => home.join(".cursor/hooks.json"),
        "opencode" => opencode_plugin_path(home),
        _ => unreachable!(),
    }
}

pub fn install(home: &Path, agent: &str, json: bool) -> Result<(), CliError> {
    let path = file_for(home, agent);
    let changed = match agent {
        "claude" => install_claude_style(&path, agent, CLAUDE_EVENTS, 10)?,
        // Codex clamps hook timeouts to 3s (warns otherwise).
        "codex" => install_claude_style(&path, agent, CODEX_EVENTS, 3)?,
        "cursor" => install_cursor(&path)?,
        "opencode" => {
            if path.is_file()
                && std::fs::read_to_string(&path)
                    .map(|s| s.contains(MARKER))
                    .unwrap_or(false)
            {
                false
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| CliError::Io(e.to_string()))?;
                }
                let content = OPENCODE_PLUGIN.replace("__SIGNALTTY_CLI__", &cli_path());
                std::fs::write(&path, content).map_err(|e| CliError::Io(e.to_string()))?;
                true
            }
        }
        _ => return Err(CliError::Usage(format!("unknown agent '{agent}'"))),
    };
    if json {
        println!(
            "{}",
            serde_json::json!({"agent": agent, "file": path.display().to_string(), "changed": changed})
        );
    } else if changed {
        println!("installed {agent} hooks → {}", path.display());
    } else {
        println!("{agent} hooks already installed ({})", path.display());
    }
    Ok(())
}

pub fn uninstall(home: &Path, agent: &str, json: bool) -> Result<(), CliError> {
    let path = file_for(home, agent);
    let changed = match agent {
        "claude" | "codex" => uninstall_claude_style(&path)?,
        "cursor" => uninstall_cursor(&path)?,
        "opencode" => {
            let ours = path.is_file()
                && std::fs::read_to_string(&path)
                    .map(|s| s.contains(MARKER))
                    .unwrap_or(false);
            if ours {
                std::fs::remove_file(&path).map_err(|e| CliError::Io(e.to_string()))?;
                true
            } else {
                false
            }
        }
        _ => return Err(CliError::Usage(format!("unknown agent '{agent}'"))),
    };
    if json {
        println!(
            "{}",
            serde_json::json!({"agent": agent, "file": path.display().to_string(), "changed": changed})
        );
    } else if changed {
        println!("uninstalled {agent} hooks ({})", path.display());
    } else {
        println!("{agent} hooks not installed");
    }
    Ok(())
}

pub fn status(home: &Path, agents: &[String], json: bool) -> Result<(), CliError> {
    let list: Vec<&str> = if agents.is_empty() {
        valid_agents().to_vec()
    } else {
        agents.iter().map(|s| s.as_str()).collect()
    };
    let mut out = serde_json::json!({});
    for agent in &list {
        if !valid_agents().contains(agent) {
            return Err(CliError::Usage(format!("unknown agent '{agent}'")));
        }
        let path = file_for(home, agent);
        let installed = if *agent == "opencode" {
            path.is_file()
                && std::fs::read_to_string(&path)
                    .map(|s| s.contains(MARKER))
                    .unwrap_or(false)
        } else {
            path.is_file() && contains_marker(&read_json(&path))
        };
        out[agent] = serde_json::json!({
            "installed": installed,
            "file": path.display().to_string(),
        });
    }
    // Detection overlays (data, not code): manifests in the agents dir.
    let agents_dir = std::env::var("SIGNALTTY_AGENTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| signaltty_core::paths::agents_dir());
    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        files.sort();
        for path in files {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let entry = match std::fs::read_to_string(&path) {
                Ok(text) => match signaltty_agent::parse_manifest(&text) {
                    Ok(m) => serde_json::json!({
                        "file": path.display().to_string(),
                        "kind": m.kind().map(|k| k.as_str().to_string()).unwrap_or_default(),
                        "ok": true,
                    }),
                    Err(e) => serde_json::json!({
                        "file": path.display().to_string(),
                        "ok": false, "error": e,
                    }),
                },
                Err(e) => serde_json::json!({
                    "file": path.display().to_string(),
                    "ok": false, "error": e.to_string(),
                }),
            };
            manifests.push(serde_json::json!({"name": name, "manifest": entry}));
        }
    }
    out["manifests"] = serde_json::Value::Array(manifests);
    if json {
        println!("{out}");
    } else {
        for agent in &list {
            let e = &out[agent];
            println!(
                "{agent}: {} ({})",
                if e["installed"].as_bool().unwrap() {
                    "installed"
                } else {
                    "not installed"
                },
                e["file"].as_str().unwrap()
            );
        }
        let manifests = out["manifests"].as_array().cloned().unwrap_or_default();
        if manifests.is_empty() {
            println!("manifests: none ({})", agents_dir.display());
        }
        for m in &manifests {
            let inner = &m["manifest"];
            if inner["ok"].as_bool().unwrap_or(false) {
                println!(
                    "manifest {}: kind={} ({})",
                    m["name"].as_str().unwrap_or("?"),
                    inner["kind"].as_str().unwrap_or("?"),
                    inner["file"].as_str().unwrap_or("?"),
                );
            } else {
                println!(
                    "manifest {}: BROKEN: {} ({})",
                    m["name"].as_str().unwrap_or("?"),
                    inner["error"].as_str().unwrap_or("?"),
                    inner["file"].as_str().unwrap_or("?"),
                );
            }
        }
    }
    Ok(())
}
