//! Executable plugins: packages with a `plugin.toml` manifest and
//! executable entrypoints. Plugins talk to the server through the same
//! stable CLI/socket API as every other client.
//!
//! Capabilities in v1:
//! - `[[hook]]`: run a command when a matching server event fires.
//!   The event envelope is JSON on stdin; reserved `SIGNALTTY_*` env
//!   bindings (socket, event, plugin dir/name) always win over `hook.env`;
//!   cwd is the plugin directory.
//! - `[[command]]`: named runnable entrypoints (`signaltty plugin run`).
//!
//! Plugins run with the user's permissions and must be trusted.
//! No sandboxing in v1 (see docs/13-plugins.md).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "plugin.toml";
pub const DEFAULT_HOOK_TIMEOUT_SECS: u64 = 10;
pub const MAX_HOOK_TIMEOUT_SECS: u64 = 300;
pub const MAX_CONCURRENT_HOOKS: usize = 8;

#[derive(Debug, Clone, thiserror::Error)]
pub enum PluginError {
    #[error("{0}")]
    Invalid(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for PluginError {
    fn from(e: std::io::Error) -> PluginError {
        PluginError::Io(e.to_string())
    }
}

// ---- manifest ----

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMeta {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hook {
    /// Event globs: exact names, `prefix.*`, or `*`.
    pub events: Vec<String>,
    /// argv; argv[0] with a `/` resolves against the plugin dir.
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_timeout() -> u64 {
    DEFAULT_HOOK_TIMEOUT_SECS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCommand {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub run: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub plugin: PluginMeta,
    #[serde(default)]
    pub hook: Vec<Hook>,
    #[serde(default)]
    pub command: Vec<PluginCommand>,
}

fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Manifest {
    pub fn validate(&self) -> Result<(), PluginError> {
        let bad = |m: String| PluginError::Invalid(m);
        if !valid_name(&self.plugin.name) {
            return Err(bad(format!(
                "plugin name {:?} must match [a-z0-9_-]{{1,64}}",
                self.plugin.name
            )));
        }
        for (i, h) in self.hook.iter().enumerate() {
            if h.events.is_empty() {
                return Err(bad(format!("hook {i}: 'events' must not be empty")));
            }
            if h.command.is_empty() {
                return Err(bad(format!("hook {i}: 'command' must not be empty")));
            }
            if !(1..=MAX_HOOK_TIMEOUT_SECS).contains(&h.timeout_secs) {
                return Err(bad(format!(
                    "hook {i}: 'timeout_secs' must be between 1 and {MAX_HOOK_TIMEOUT_SECS}"
                )));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for c in &self.command {
            if !valid_name(&c.name) {
                return Err(bad(format!(
                    "command name {:?} must match [a-z0-9_-]{{1,64}}",
                    c.name
                )));
            }
            if c.run.is_empty() {
                return Err(bad(format!(
                    "command {:?}: 'run' must not be empty",
                    c.name
                )));
            }
            if !seen.insert(c.name.clone()) {
                return Err(bad(format!("duplicate command {:?}", c.name)));
            }
        }
        Ok(())
    }

    pub fn timeout_clamped(&self, hook: &Hook) -> u64 {
        let _ = self;
        hook.timeout_secs.min(MAX_HOOK_TIMEOUT_SECS)
    }
}

pub fn parse_manifest(text: &str) -> Result<Manifest, PluginError> {
    let m: Manifest = toml::from_str(text)
        .map_err(|e| PluginError::Invalid(format!("bad {MANIFEST_FILE}: {e}")))?;
    m.validate()?;
    Ok(m)
}

// ---- loading ----

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub stats: Vec<HookStats>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct HookStats {
    pub runs: u64,
    pub errors: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginFailure {
    pub dir: String,
    pub error: String,
}

/// Load all `<dir>/*/plugin.toml` packages. Per-package failures are
/// collected, never fatal to siblings.
pub fn load_dir(dir: &Path) -> (Vec<LoadedPlugin>, Vec<PluginFailure>) {
    let mut plugins = Vec::new();
    let mut failures = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (plugins, failures),
        Err(e) => {
            failures.push(PluginFailure {
                dir: dir.display().to_string(),
                error: format!("cannot read plugin dir: {e}"),
            });
            return (plugins, failures);
        }
    };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    dirs.sort();
    for sub in dirs {
        if !sub.is_dir() {
            continue;
        }
        let manifest_path = sub.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            continue;
        }
        let name = sub
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        match std::fs::read_to_string(&manifest_path) {
            Ok(text) => match parse_manifest(&text) {
                Ok(m) => {
                    if m.plugin.name != name {
                        failures.push(PluginFailure {
                            dir: name,
                            error: format!(
                                "manifest name {:?} must match directory name",
                                m.plugin.name
                            ),
                        });
                        continue;
                    }
                    let stats = vec![HookStats::default(); m.hook.len()];
                    plugins.push(LoadedPlugin {
                        dir: sub,
                        manifest: m,
                        stats,
                    });
                }
                Err(e) => failures.push(PluginFailure {
                    dir: name,
                    error: e.to_string(),
                }),
            },
            Err(e) => failures.push(PluginFailure {
                dir: name,
                error: format!("cannot read {MANIFEST_FILE}: {e}"),
            }),
        }
    }
    (plugins, failures)
}

// ---- argv resolution ----

/// Resolve hook/command argv[0]: absolute stays, `./`- or `x/`-relative
/// resolves against the plugin dir, bare names use PATH.
pub fn resolve_argv(plugin_dir: &Path, argv: &[String]) -> Vec<String> {
    let mut out = argv.to_vec();
    if let Some(first) = out.first_mut() {
        if first.contains('/') {
            let p = Path::new(first.as_str());
            if p.is_relative() {
                *first = plugin_dir.join(p).display().to_string();
            }
        }
    }
    out
}

// ---- registry + dispatch ----

struct RegistryState {
    generation: u64,
    plugins: Vec<LoadedPlugin>,
    failures: Vec<PluginFailure>,
}

struct RegistryInner {
    dir: PathBuf,
    state: Mutex<RegistryState>,
    semaphore: tokio::sync::Semaphore,
}

#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<RegistryInner>,
}

impl PluginRegistry {
    pub fn load(dir: PathBuf) -> PluginRegistry {
        let (plugins, failures) = load_dir(&dir);
        for f in &failures {
            tracing::warn!("plugin {:?} failed to load: {}", f.dir, f.error);
        }
        tracing::info!("loaded {} plugin(s) from {}", plugins.len(), dir.display());
        PluginRegistry {
            inner: Arc::new(RegistryInner {
                dir,
                state: Mutex::new(RegistryState {
                    generation: 0,
                    plugins,
                    failures,
                }),
                semaphore: tokio::sync::Semaphore::new(MAX_CONCURRENT_HOOKS),
            }),
        }
    }

    pub fn reload(&self) {
        let (plugins, failures) = load_dir(&self.inner.dir);
        let mut state = self.inner.state.lock().unwrap();
        state.generation += 1;
        state.plugins = plugins;
        state.failures = failures;
    }

    /// Snapshot for `plugin.list`.
    pub fn status(&self) -> serde_json::Value {
        let s = self.inner.state.lock().unwrap();
        serde_json::json!({
            "dir": self.inner.dir.display().to_string(),
            "plugins": s.plugins.iter().map(|p| serde_json::json!({
                "name": p.manifest.plugin.name,
                "version": p.manifest.plugin.version,
                "description": p.manifest.plugin.description,
                "dir": p.dir.display().to_string(),
                "hooks": p.manifest.hook.iter().zip(p.stats.iter()).map(|(h, st)| serde_json::json!({
                    "events": h.events,
                    "command": h.command,
                    "timeout_secs": h.timeout_secs,
                    "runs": st.runs,
                    "errors": st.errors,
                    "last_error": st.last_error,
                    "last_run": st.last_run,
                })).collect::<Vec<_>>(),
                "commands": p.manifest.command.iter().map(|c| serde_json::json!({
                    "name": c.name,
                    "description": c.description,
                    "run": c.run,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "failures": s.failures,
        })
    }

    /// Commands of one plugin (for `plugin run`).
    pub fn commands(&self, plugin: &str) -> Option<(PathBuf, Vec<PluginCommand>)> {
        let s = self.inner.state.lock().unwrap();
        s.plugins
            .iter()
            .find(|p| p.manifest.plugin.name == plugin)
            .map(|p| (p.dir.clone(), p.manifest.command.clone()))
    }

    /// Fire matching hooks for an event. Never blocks the caller long:
    /// each hook runs in its own task under a concurrency cap.
    pub async fn dispatch(&self, name: &str, envelope: &serde_json::Value, socket: &Path) {
        let (generation, targets) = {
            let s = self.inner.state.lock().unwrap();
            let mut out = Vec::new();
            for (pi, p) in s.plugins.iter().enumerate() {
                for (hi, h) in p.manifest.hook.iter().enumerate() {
                    if h.events
                        .iter()
                        .any(|g| signaltty_proto::glob_matches(g, name))
                    {
                        out.push((pi, hi, p.clone(), h.clone()));
                    }
                }
            }
            (s.generation, out)
        };
        for (pi, hi, plugin, hook) in targets {
            let this = self.clone();
            let envelope = envelope.clone();
            let socket = socket.to_path_buf();
            let name = name.to_string();
            tokio::spawn(async move {
                let _permit = this.inner.semaphore.acquire().await.unwrap();
                let err = run_hook(&plugin, &hook, &name, &envelope, &socket)
                    .await
                    .err();
                // Indices identify hooks only within the captured generation.
                // Old completions must not repopulate stats reset by reload.
                let mut s = this.inner.state.lock().unwrap();
                if s.generation != generation {
                    return;
                }
                if let Some(st) = s.plugins.get_mut(pi).and_then(|p| p.stats.get_mut(hi)) {
                    st.runs += 1;
                    st.last_run = Some(Utc::now());
                    if let Some(e) = err {
                        st.errors += 1;
                        st.last_error = Some(e);
                    }
                }
            });
        }
    }
}

async fn run_hook(
    plugin: &LoadedPlugin,
    hook: &Hook,
    event: &str,
    envelope: &serde_json::Value,
    socket: &Path,
) -> Result<(), String> {
    let argv = resolve_argv(&plugin.dir, &hook.command);
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.current_dir(&plugin.dir);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // hook.env is additive. Apply it first, then force the reserved
    // SIGNALTTY_* bindings so a mistaken or malicious manifest cannot
    // redirect hooks at another socket or forge event identity.
    for (k, v) in &hook.env {
        cmd.env(k, v);
    }
    cmd.env("SIGNALTTY_SOCKET", socket);
    cmd.env("SIGNALTTY_EVENT", event);
    cmd.env("SIGNALTTY_PLUGIN_DIR", &plugin.dir);
    cmd.env("SIGNALTTY_PLUGIN_NAME", &plugin.manifest.plugin.name);
    let body = serde_json::to_vec(envelope).unwrap();
    let timeout = std::time::Duration::from_secs(hook.timeout_secs.min(MAX_HOOK_TIMEOUT_SECS));
    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let fail = |msg: String| {
        tracing::warn!(
            "plugin {:?} hook for {event} failed: {msg}",
            plugin.manifest.plugin.name
        );
        truncate(&msg, 500)
    };
    // Feed stdin, stdout and stderr concurrently with the wait, all under
    // the hook timeout: a hook that ignores stdin, or fills one pipe while
    // we read the other, must not wedge the runner (and its permit).
    let stdin = child.stdin.take();
    let writer = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = stdin {
            // The hook is free not to read its stdin (EPIPE is fine);
            // its exit status is what decides success.
            let _ = stdin.write_all(&body).await;
        }
    });
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let mut drain = tokio::spawn(async move {
        tokio::join!(
            drain_capped(stdout, 0),
            drain_capped(stderr, STDERR_CAPTURE_BYTES)
        )
    });
    let deadline = tokio::time::Instant::now() + timeout;
    let status = match tokio::time::timeout_at(deadline, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            writer.abort();
            drain.abort();
            return Err(fail(format!("wait failed: {e}")));
        }
        Err(_) => {
            child.kill().await.ok();
            let _ = child.wait().await;
            writer.abort();
            drain.abort();
            return Err(fail(format!("timed out after {}s", timeout.as_secs())));
        }
    };
    // A backgrounded grandchild can keep the pipes open; don't wait past
    // the hook's own deadline for them.
    let err_bytes = match tokio::time::timeout_at(deadline, &mut drain).await {
        Ok(Ok((_, e))) => e,
        _ => Vec::new(),
    };
    drain.abort();
    writer.abort();
    if status.success() {
        tracing::debug!(
            "plugin {:?} hook ok for {event}",
            plugin.manifest.plugin.name
        );
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&err_bytes);
        Err(fail(format!("exit {status}: {}", stderr.trim())))
    }
}

/// Stderr bytes kept for a failed hook's `last_error` (itself cut to 500
/// chars). Stdout is never kept.
const STDERR_CAPTURE_BYTES: usize = 4096;

/// Read `pipe` to EOF so the hook never blocks on a full pipe, keeping only
/// the first `cap` bytes: a chatty hook must not grow server memory for
/// its whole timeout.
async fn drain_capped(pipe: Option<impl tokio::io::AsyncRead + Unpin>, cap: usize) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut kept = Vec::new();
    let Some(mut pipe) = pipe else {
        return kept;
    };
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
    }
    kept
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[plugin]
name = "demo"
version = "0.1.0"
description = "demo plugin"

[[hook]]
events = ["agent.done", "attention.*"]
command = ["./hook.sh"]
timeout_secs = 5

[hook.env]
OUT = "/tmp/x.log"

[[command]]
name = "fanout"
description = "spawn panes"
run = ["./fanout.sh"]
"#;

    #[test]
    fn parse_valid_manifest() {
        let m = parse_manifest(GOOD).unwrap();
        assert_eq!(m.plugin.name, "demo");
        assert_eq!(m.hook.len(), 1);
        assert_eq!(m.hook[0].events, vec!["agent.done", "attention.*"]);
        assert_eq!(m.hook[0].timeout_secs, 5);
        assert_eq!(m.hook[0].env.get("OUT").unwrap(), "/tmp/x.log");
        assert_eq!(m.command.len(), 1);
    }

    #[test]
    fn rejects_timeout_above_documented_max() {
        let err = parse_manifest(
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = [\"true\"]\ntimeout_secs = 301\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("between 1 and 300"), "{err}");
        // Boundary still loads; runtime no longer needs to silently clamp.
        let m = parse_manifest(
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = [\"true\"]\ntimeout_secs = 300\n",
        )
        .unwrap();
        assert_eq!(m.hook[0].timeout_secs, MAX_HOOK_TIMEOUT_SECS);
    }

    #[test]
    fn defaults_apply() {
        let m = parse_manifest(
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = [\"true\"]\n",
        )
        .unwrap();
        assert_eq!(m.hook[0].timeout_secs, DEFAULT_HOOK_TIMEOUT_SECS);
        assert!(m.command.is_empty());
    }

    #[test]
    fn rejects_bad_manifests() {
        for bad in [
            "[plugin]\nname = \"Bad Name\"\n",           // name charset
            "[plugin]\nname = \"\"\n",                   // empty name
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = []\ncommand = [\"true\"]\n", // empty events
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = []\n", // empty command
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = [\"true\"]\ntimeout_secs = 0\n", // zero timeout
            "[plugin]\nname = \"x\"\n[[hook]]\nevents = [\"*\"]\ncommand = [\"true\"]\ntimeout_secs = 301\n", // above max
            "[plugin]\nname = \"x\"\n[[command]]\nname = \"UP\"\nrun = [\"x\"]\n", // command name charset
            "[plugin]\nname = \"x\"\n[[command]]\nname = \"a\"\nrun = [\"x\"]\n[[command]]\nname = \"a\"\nrun = [\"y\"]\n", // dup command
            "not toml [[[\n",
        ] {
            assert!(parse_manifest(bad).is_err(), "should reject: {bad:?}");
        }
    }

    #[test]
    fn dir_name_must_match_manifest() {
        let base = std::env::temp_dir().join(format!("plugtest-{}", std::process::id()));
        let sub = base.join("other");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(MANIFEST_FILE), "[plugin]\nname = \"mismatch\"\n").unwrap();
        let (plugins, failures) = load_dir(&base);
        assert!(plugins.is_empty());
        assert_eq!(failures.len(), 1);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn argv_resolution() {
        let dir = Path::new("/p/demo");
        assert_eq!(
            resolve_argv(dir, &["./h.sh".to_string(), "a".to_string()]),
            vec!["/p/demo/./h.sh".to_string(), "a".to_string()]
        );
        assert_eq!(
            resolve_argv(dir, &["/bin/true".to_string()]),
            vec!["/bin/true".to_string()]
        );
        assert_eq!(
            resolve_argv(dir, &["true".to_string()]),
            vec!["true".to_string()]
        );
    }

    fn sh_plugin(script: &str, timeout_secs: u64) -> (LoadedPlugin, Hook) {
        let hook = Hook {
            events: vec!["*".into()],
            command: vec!["sh".into(), "-c".into(), script.into()],
            timeout_secs,
            env: HashMap::new(),
        };
        let plugin = LoadedPlugin {
            dir: std::env::temp_dir(),
            manifest: Manifest {
                plugin: PluginMeta {
                    name: "pipes".into(),
                    version: String::new(),
                    description: None,
                },
                hook: vec![hook.clone()],
                command: Vec::new(),
            },
            stats: vec![HookStats::default()],
        };
        (plugin, hook)
    }

    #[tokio::test]
    async fn hook_may_ignore_large_stdin() {
        // Bigger than a pipe buffer, never read: must not block or fail.
        let (plugin, hook) = sh_plugin("sleep 0.2; exit 0", 5);
        let env = serde_json::json!({ "blob": "x".repeat(512 * 1024) });
        let r = run_hook(
            &plugin,
            &hook,
            "agent.done",
            &env,
            Path::new("/nonexistent"),
        )
        .await;
        assert_eq!(r, Ok(()));
    }

    #[tokio::test]
    async fn chatty_stderr_does_not_wedge_stdout_drain() {
        // Fills stderr while stdout is still open; serial drains deadlock here.
        let (plugin, hook) = sh_plugin("head -c 262144 /dev/zero >&2; echo ok", 3);
        let env = serde_json::json!({});
        let r = run_hook(
            &plugin,
            &hook,
            "agent.done",
            &env,
            Path::new("/nonexistent"),
        )
        .await;
        assert_eq!(r, Ok(()));
    }

    #[tokio::test]
    async fn hook_output_is_drained_but_only_a_bounded_prefix_is_kept() {
        // A chatty hook (`yes`, a verbose tool) must not grow server memory
        // for its whole timeout: every byte is read, few are kept.
        let big = vec![b'x'; 4 * 1024 * 1024];
        let mut pipe: &[u8] = &big;
        let kept = drain_capped(Some(&mut pipe), STDERR_CAPTURE_BYTES).await;
        assert_eq!(kept.len(), STDERR_CAPTURE_BYTES);
        assert!(pipe.is_empty(), "the pipe is drained to EOF");
        let mut pipe: &[u8] = &big;
        assert!(drain_capped(Some(&mut pipe), 0).await.is_empty());
        assert!(pipe.is_empty());
    }

    #[tokio::test]
    async fn failing_hook_reports_stderr() {
        let (plugin, hook) = sh_plugin("cat >/dev/null; echo boom >&2; exit 3", 5);
        let env = serde_json::json!({ "k": "v" });
        let err = run_hook(
            &plugin,
            &hook,
            "agent.done",
            &env,
            Path::new("/nonexistent"),
        )
        .await
        .unwrap_err();
        assert!(err.contains("boom"), "{err}");
    }

    #[tokio::test]
    async fn reload_does_not_attribute_in_flight_stats_by_index() {
        let base = std::env::temp_dir().join(format!(
            "plug-reload-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let alpha = base.join("alpha");
        let bravo = base.join("bravo");
        std::fs::create_dir_all(&alpha).unwrap();
        std::fs::create_dir_all(&bravo).unwrap();
        std::fs::write(
            alpha.join(MANIFEST_FILE),
            r#"[plugin]
name = "alpha"
version = "0.1.0"
[[hook]]
events = ["agent.done"]
command = ["sh", "-c", "sleep 0.4; exit 0"]
timeout_secs = 5
"#,
        )
        .unwrap();
        std::fs::write(
            bravo.join(MANIFEST_FILE),
            r#"[plugin]
name = "bravo"
version = "0.1.0"
[[hook]]
events = ["other.event"]
command = ["true"]
timeout_secs = 5
"#,
        )
        .unwrap();

        let registry = PluginRegistry::load(base.clone());
        let status = registry.status();
        assert_eq!(status["plugins"][0]["name"], "alpha");
        assert_eq!(status["plugins"][1]["name"], "bravo");

        registry
            .dispatch(
                "agent.done",
                &serde_json::json!({"protocol": "signaltty/1"}),
                Path::new("/nonexistent.sock"),
            )
            .await;

        // Remove alpha so reload puts bravo at index 0. An index-based
        // stats write from the in-flight alpha hook would land on bravo.
        std::fs::remove_dir_all(&alpha).unwrap();
        registry.reload();
        let after = registry.status();
        assert_eq!(after["plugins"].as_array().unwrap().len(), 1);
        assert_eq!(after["plugins"][0]["name"], "bravo");
        assert_eq!(after["plugins"][0]["hooks"][0]["runs"], 0);

        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let final_status = registry.status();
        assert_eq!(
            final_status["plugins"][0]["hooks"][0]["runs"], 0,
            "in-flight alpha hook must not credit bravo after reload"
        );
        assert_eq!(final_status["plugins"][0]["name"], "bravo");
        std::fs::remove_dir_all(&base).ok();
    }
    #[tokio::test]
    async fn duplicate_hooks_keep_separate_stats() {
        let base = std::env::temp_dir().join(format!("plug-duplicates-{}", std::process::id()));
        let dir = base.join("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            r#"[plugin]
name = "alpha"
[[hook]]
events = ["agent.done"]
command = ["true"]
[[hook]]
events = ["agent.done"]
command = ["true"]
"#,
        )
        .unwrap();
        let registry = PluginRegistry::load(base.clone());
        registry
            .dispatch(
                "agent.done",
                &serde_json::json!({}),
                Path::new("/unused.sock"),
            )
            .await;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let status = registry.status();
                let hooks = status["plugins"][0]["hooks"].as_array().unwrap();
                if hooks
                    .iter()
                    .map(|h| h["runs"].as_u64().unwrap())
                    .sum::<u64>()
                    == 2
                {
                    assert_eq!(hooks[0]["runs"], 1);
                    assert_eq!(hooks[1]["runs"], 1);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn reload_discards_old_generation_completions() {
        let base = std::env::temp_dir().join(format!("plug-generation-{}", std::process::id()));
        let dir = base.join("alpha");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(MANIFEST_FILE),
            r#"[plugin]
name = "alpha"
[[hook]]
events = ["agent.done"]
command = ["true"]
"#,
        )
        .unwrap();
        let registry = PluginRegistry::load(base.clone());
        let permits = registry
            .inner
            .semaphore
            .acquire_many(MAX_CONCURRENT_HOOKS as u32)
            .await
            .unwrap();
        registry
            .dispatch(
                "agent.done",
                &serde_json::json!({}),
                Path::new("/unused.sock"),
            )
            .await;
        tokio::task::yield_now().await;
        registry.reload();
        drop(permits);
        let _finished = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            registry
                .inner
                .semaphore
                .acquire_many(MAX_CONCURRENT_HOOKS as u32),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(registry.status()["plugins"][0]["hooks"][0]["runs"], 0);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn reserved_signaltty_env_wins_over_hook_env() {
        let log = std::env::temp_dir().join(format!(
            "signaltty-hook-env-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let script = format!(
            concat!(
                "printf '%s\\n' ",
                "\"$SIGNALTTY_SOCKET\" \"$SIGNALTTY_EVENT\" ",
                "\"$SIGNALTTY_PLUGIN_NAME\" \"$SIGNALTTY_PLUGIN_DIR\" ",
                "\"$CUSTOM_OK\" > '{}'"
            ),
            log.display()
        );
        let (plugin, mut hook) = sh_plugin(&script, 5);
        hook.env
            .insert("SIGNALTTY_SOCKET".into(), "/forged.sock".into());
        hook.env
            .insert("SIGNALTTY_EVENT".into(), "forged.event".into());
        hook.env
            .insert("SIGNALTTY_PLUGIN_NAME".into(), "forged".into());
        hook.env
            .insert("SIGNALTTY_PLUGIN_DIR".into(), "/forged".into());
        hook.env.insert("CUSTOM_OK".into(), "yes".into());
        let socket = Path::new("/tmp/real-signaltty.sock");
        run_hook(&plugin, &hook, "agent.done", &serde_json::json!({}), socket)
            .await
            .unwrap();
        let body = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(
            lines,
            [
                "/tmp/real-signaltty.sock",
                "agent.done",
                "pipes",
                plugin.dir.to_str().unwrap(),
                "yes",
            ],
            "{body}"
        );
        std::fs::remove_file(&log).ok();
    }
}
