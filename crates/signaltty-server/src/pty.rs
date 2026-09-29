//! PTY ownership: spawn, pumps, resize, signals, exit reaping.
//! The server — never clients — owns all of this. See docs/04.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use chrono::Utc;
use portable_pty::{native_pty_system, CommandBuilder, PtySize as PtySizeRaw};
use tokio::sync::broadcast;

use signaltty_core::model::{LiveState, PtySize, RestoreState};
use signaltty_core::state::{Attention, Lifecycle};
use signaltty_term::{HeadlessBackend, OscEvent, OscScanner, TerminalBackend};

use crate::store::{SharedStore, StoredEvent};

/// Env vars a client may override at spawn. Everything else comes from
/// the server environment, minus the blocklist.
const ENV_ALLOW: &[&str] = &[
    "TERM",
    "COLORTERM",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "SHELL",
    "SIGNALTTY_PANE",
];
const ENV_PREFIX_ALLOW: &[&str] = &["SIGNALTTY_", "CLAUDE_", "CODEX_", "OPENCODE_", "CURSOR_"];
const ENV_BLOCK: &[&str] = &["LD_PRELOAD", "LD_LIBRARY_PATH", "SIGNALTTY_SOCKET"];

pub struct PtyHandle {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child_pid: Option<u32>,
}

pub struct SpawnRequest {
    pub pane_id: String,
    pub cwd: String,
    pub argv: Vec<String>,
    pub env: HashMap<String, String>,
    pub size: PtySize,
    pub socket_path: String,
}

#[derive(Clone)]
pub struct PtyManager {
    store: SharedStore,
    bcast: broadcast::Sender<StoredEvent>,
    terms: Arc<Mutex<HeadlessBackend>>,
    output_offsets: Arc<Mutex<HashMap<String, u64>>>,
    handles: Arc<Mutex<HashMap<String, PtyHandle>>>,
    viewers: Arc<Mutex<HashMap<String, usize>>>,
    persist_pending: Arc<AtomicBool>,
    integration_home: Option<std::path::PathBuf>,
}

impl PtyManager {
    pub fn new(
        store: SharedStore,
        bcast: broadcast::Sender<StoredEvent>,
        persist_pending: Arc<AtomicBool>,
    ) -> PtyManager {
        PtyManager {
            store,
            bcast,
            terms: Arc::new(Mutex::new(HeadlessBackend::new())),
            output_offsets: Arc::new(Mutex::new(HashMap::new())),
            handles: Arc::new(Mutex::new(HashMap::new())),
            viewers: Arc::new(Mutex::new(HashMap::new())),
            persist_pending,
            integration_home: None,
        }
    }

    pub fn with_integration_home(mut self, home: Option<std::path::PathBuf>) -> Self {
        self.integration_home = home;
        self
    }

    fn prepare_integration(&self, req: &SpawnRequest) -> serde_json::Value {
        use signaltty_integration::{reporter_executable, Hooks};
        let kind = signaltty_agent::detect_kind(&req.argv);
        let agent = kind.as_str();
        if !signaltty_integration::AGENTS.contains(&agent) {
            return serde_json::Value::Null;
        }
        let user_sources_excluded = req.argv.iter().enumerate().any(|(i, arg)| {
            let sources = arg.strip_prefix("--setting-sources=").or_else(|| {
                (arg == "--setting-sources")
                    .then(|| req.argv.get(i + 1).map(String::as_str))
                    .flatten()
            });
            sources.is_some_and(|s| !s.split(',').any(|source| source == "user"))
        });
        let disabled = (agent == "claude"
            && (req.argv.iter().any(|a| a == "--bare") || user_sources_excluded))
            || (agent == "opencode" && req.argv.iter().any(|a| a == "--pure"));
        if disabled {
            return serde_json::json!({"agent": agent, "status": "disabled", "changed": false,
                "notice": format!("{agent}: this launch disables user hooks/plugins; status integration is unavailable.")});
        }
        let outcome = reporter_executable()
            .and_then(|cli| Hooks::from_env(self.integration_home.as_deref(), cli))
            .map(|hooks| hooks.with_overrides(&req.env, std::path::Path::new(&req.cwd)))
            .and_then(|hooks| hooks.install(agent));
        match outcome {
            Ok(report) => {
                let mut result = serde_json::to_value(report).unwrap();
                result["status"] = serde_json::json!("configured");
                result
            }
            Err(e) => serde_json::json!({"agent": agent, "status": "error", "changed": false,
                "notice": format!("{agent} status setup failed: {e}. The terminal remains available.")}),
        }
    }

    pub fn terms(&self) -> Arc<Mutex<HeadlessBackend>> {
        self.terms.clone()
    }

    /// The replayable terminal state and its covered byte offset share a lock.
    pub fn snapshot(&self, pane_id: &str) -> (Vec<u8>, u64) {
        let terms = self.terms.lock().unwrap();
        let offsets = self.output_offsets.lock().unwrap();
        (
            terms.screen_state(pane_id),
            offsets.get(pane_id).copied().unwrap_or(0),
        )
    }

    pub fn mark_persist(&self) {
        self.persist_pending.store(true, Ordering::Relaxed);
    }

    pub fn spawn(&self, req: SpawnRequest) -> Result<serde_json::Value, String> {
        if req.argv.is_empty() {
            return Err("argv must not be empty".to_string());
        }
        let mut integration = self.prepare_integration(&req);
        let (argv, runtime_notice) = crate::codex::prepare(&req.argv, &req.cwd);
        if let Some(notice) = runtime_notice {
            if integration.is_object() {
                let previous = integration["notice"].as_str().unwrap_or("");
                integration["notice"] = serde_json::json!(format!("{notice} {previous}").trim());
                if integration["status"] != "error" {
                    integration["status"] = serde_json::json!("disabled");
                }
            }
        }
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySizeRaw {
                rows: req.size.rows,
                cols: req.size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("openpty failed: {e:#}"))?;

        let mut cmd = CommandBuilder::new(&argv[0]);
        if argv.len() > 1 {
            cmd.args(&argv[1..]);
        }
        cmd.cwd(&req.cwd);
        // Filtered environment: server env minus blocklist, plus
        // allowlisted overrides, plus signaltty context.
        for (k, v) in std::env::vars() {
            if !ENV_BLOCK.contains(&k.as_str()) {
                cmd.env(&k, &v);
            }
        }
        for (k, v) in &req.env {
            if ENV_BLOCK.contains(&k.as_str()) {
                continue;
            }
            if ENV_ALLOW.contains(&k.as_str()) || ENV_PREFIX_ALLOW.iter().any(|p| k.starts_with(p))
            {
                cmd.env(k, v);
            }
        }
        if std::env::var("TERM").is_err() && !req.env.contains_key("TERM") {
            cmd.env("TERM", "xterm-256color");
        }
        cmd.env("SIGNALTTY_PANE", &req.pane_id);
        cmd.env("SIGNALTTY_SOCKET", &req.socket_path);

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("spawn failed: {e:#}"))?;
        let child_pid = child.process_id();
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("pty reader failed: {e:#}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("pty writer failed: {e:#}"))?;

        self.terms
            .lock()
            .unwrap()
            .ensure_surface(&req.pane_id, req.size.cols, req.size.rows);
        self.handles.lock().unwrap().insert(
            req.pane_id.clone(),
            PtyHandle {
                master: pair.master,
                writer,
                child_pid,
            },
        );

        // Reader + reaper thread. Blocking I/O by design; one per pane.
        let pump = self.clone();
        let pane_id = req.pane_id.clone();
        std::thread::Builder::new()
            .name(format!("pty-pump-{pane_id}"))
            .spawn(move || pump.run(pane_id, reader, child))
            .map_err(|e| format!("pump thread failed: {e}"))?;

        Ok(integration)
    }

    fn run(
        &self,
        pane_id: String,
        mut reader: Box<dyn Read + Send>,
        mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ) {
        let mut buf = [0u8; 8192];
        let mut scanner = OscScanner::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.on_output(&pane_id, &buf[..n], &mut scanner),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break, // EIO after child death is normal.
            }
        }
        let code = child.wait().ok().map(|s| s.exit_code() as i32);
        self.on_exit(&pane_id, code);
    }

    fn on_output(&self, pane_id: &str, data: &[u8], scanner: &mut OscScanner) {
        let output_offset = {
            let mut terms = self.terms.lock().unwrap();
            let mut offsets = self.output_offsets.lock().unwrap();
            terms.feed_output(pane_id, data);
            let offset = offsets.entry(pane_id.to_string()).or_default();
            *offset += data.len() as u64;
            *offset
        };
        let broadcast_data = {
            let viewers = self.viewers.lock().unwrap();
            viewers.get(pane_id).copied().unwrap_or(0) > 0
        };
        for ev in scanner.push(data) {
            self.on_osc_event(pane_id, ev);
        }
        // Touch activity (no event, no persist: too hot).
        if let Ok(mut s) = self.store.try_write() {
            if let Some(p) = s.panes.get_mut(pane_id) {
                p.last_activity_at = Utc::now();
            }
        }
        if broadcast_data {
            let seq = self.store.write().map(|mut s| s.next_seq()).unwrap_or(0);
            let _ = self.bcast.send(StoredEvent {
                seq,
                name: signaltty_proto::event::PTY_DATA.to_string(),
                payload: serde_json::json!({
                    "pane_id": pane_id,
                    "data_b64": base64::engine::general_purpose::STANDARD.encode(data),
                    "output_offset": output_offset,
                }),
            });
        }
    }

    fn on_osc_event(&self, pane_id: &str, ev: OscEvent) {
        match ev {
            OscEvent::Title(title) => {
                let title = signaltty_term::sanitize_notification_text(&title, 200);
                let changed = {
                    let mut s = self.store.write().unwrap();
                    match s.panes.get_mut(pane_id) {
                        Some(p) if p.title != title => {
                            p.title = title.clone();
                            p.last_activity_at = Utc::now();
                            Some(s.emit(
                                signaltty_proto::event::PANE_UPDATED,
                                serde_json::json!({"pane_id": pane_id, "title": title}),
                            ))
                        }
                        _ => None,
                    }
                };
                if let Some(ev) = changed {
                    let _ = self.bcast.send(ev);
                    self.mark_persist();
                }
            }
            OscEvent::Notify {
                title,
                body,
                source,
            } => {
                crate::router::push_notification(
                    &self.store,
                    &self.bcast,
                    pane_id,
                    title.as_deref(),
                    &body,
                    signaltty_core::model::NotificationSeverity::Info,
                    source,
                );
                self.mark_persist();
            }
            OscEvent::Bell => {
                let ev = self
                    .store
                    .write()
                    .unwrap()
                    .raise_attention(pane_id, Attention::Unread);
                if let Some(ev) = ev {
                    let _ = self.bcast.send(ev);
                }
            }
        }
    }

    fn on_exit(&self, pane_id: &str, code: Option<i32>) {
        self.handles.lock().unwrap().remove(pane_id);
        // Viewers belong to connections, so they survive a child restart.
        let mut outbound = Vec::new();
        {
            let mut s = self.store.write().unwrap();
            if let Some(p) = s.panes.get_mut(pane_id) {
                p.live = LiveState::Exited { code };
                p.restore_state = RestoreState::Exited;
                p.last_activity_at = Utc::now();
                outbound.push(s.emit(
                    signaltty_proto::event::PANE_EXITED,
                    serde_json::json!({"pane_id": pane_id, "code": code}),
                ));
            }
            // One lock for the whole exit: transitions ride along.
            outbound.extend(s.set_lifecycle(pane_id, Lifecycle::Exited));
            outbound.extend(s.raise_attention(pane_id, Attention::Unread));
            // A decision outlives nothing: the bar renders read-only
            // nowhere once the child is gone.
            outbound.extend(s.clear_decision(pane_id, "pane_exited"));
        }
        for ev in outbound {
            let _ = self.bcast.send(ev);
        }
        self.mark_persist();
    }

    pub fn input(&self, pane_id: &str, data: &[u8]) -> Result<usize, String> {
        let mut handles = self.handles.lock().unwrap();
        let h = handles
            .get_mut(pane_id)
            .ok_or_else(|| "pane has no live PTY".to_string())?;
        h.writer.write_all(data).map_err(|e| e.to_string())?;
        h.writer.flush().map_err(|e| e.to_string())?;
        Ok(data.len())
    }

    pub fn resize(&self, pane_id: &str, size: PtySize) -> Result<(), String> {
        let handles = self.handles.lock().unwrap();
        if let Some(h) = handles.get(pane_id) {
            h.master
                .resize(PtySizeRaw {
                    rows: size.rows,
                    cols: size.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| e.to_string())?;
        }
        self.terms
            .lock()
            .unwrap()
            .resize(pane_id, size.cols, size.rows);
        Ok(())
    }

    pub fn signal(&self, pane_id: &str, sig: &str, group: bool) -> Result<(), String> {
        let pid = {
            let handles = self.handles.lock().unwrap();
            handles
                .get(pane_id)
                .and_then(|h| h.child_pid)
                .ok_or_else(|| "pane has no live process".to_string())?
        };
        let sig = parse_signal(sig)?;
        let target = if group {
            nix::unistd::Pid::from_raw(-(pid as i32))
        } else {
            nix::unistd::Pid::from_raw(pid as i32)
        };
        nix::sys::signal::kill(target, sig).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Kill the child (if alive) and drop PTY-side state. The store pane
    /// itself is removed by the router.
    pub fn destroy(&self, pane_id: &str, sig: Option<&str>) {
        let _ = self.signal(pane_id, sig.unwrap_or("TERM"), false);
        self.handles.lock().unwrap().remove(pane_id);
        self.viewers.lock().unwrap().remove(pane_id);
        let mut terms = self.terms.lock().unwrap();
        terms.destroy(pane_id);
        self.output_offsets.lock().unwrap().remove(pane_id);
    }

    pub fn is_live(&self, pane_id: &str) -> bool {
        self.handles.lock().unwrap().contains_key(pane_id)
    }

    pub fn add_viewer(&self, pane_id: &str) {
        *self
            .viewers
            .lock()
            .unwrap()
            .entry(pane_id.to_string())
            .or_insert(0) += 1;
    }

    pub fn remove_viewer(&self, pane_id: &str) {
        let mut v = self.viewers.lock().unwrap();
        if let Some(n) = v.get_mut(pane_id) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                v.remove(pane_id);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn viewer_count(&self, pane_id: &str) -> usize {
        self.viewers
            .lock()
            .unwrap()
            .get(pane_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn live_handles(&self) -> HashSet<String> {
        self.handles.lock().unwrap().keys().cloned().collect()
    }

    /// Snapshot of live pane child pids for hook attribution.
    pub fn child_pids(&self) -> HashMap<String, u32> {
        self.handles
            .lock()
            .unwrap()
            .iter()
            .filter_map(|(id, h)| h.child_pid.map(|pid| (id.clone(), pid)))
            .collect()
    }
}

fn parse_signal(sig: &str) -> Result<nix::sys::signal::Signal, String> {
    use nix::sys::signal::Signal::*;
    let s = sig.to_ascii_uppercase();
    let s = s.strip_prefix("SIG").unwrap_or(&s);
    match s {
        "INT" => Ok(SIGINT),
        "TERM" => Ok(SIGTERM),
        "KILL" => Ok(SIGKILL),
        "HUP" => Ok(SIGHUP),
        "QUIT" => Ok(SIGQUIT),
        "WINCH" => Ok(SIGWINCH),
        "USR1" => Ok(SIGUSR1),
        "USR2" => Ok(SIGUSR2),
        _ => Err(format!("unsupported signal: {sig}")),
    }
}
