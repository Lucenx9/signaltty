//! Snapshot persistence: atomic writes, tolerant loading, honest
//! restore states. See docs/09.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use signaltty_term::HeadlessBackend;

use signaltty_core::model::{LiveState, Notification, Pane, RestoreState, Tab, Workspace};
use signaltty_term::TerminalBackend;

use crate::config::Config;
use crate::store::SharedStore;

pub const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub saved_at: DateTime<Utc>,
    pub workspaces: Vec<Workspace>,
    pub tabs: Vec<Tab>,
    pub panes: Vec<Pane>,
    pub notifications: Vec<Notification>,
}

/// Save snapshot + scrollback tails atomically. Quick enough to call
/// directly on the shutdown path; the flusher calls it periodically.
pub fn save(
    store: &SharedStore,
    terms: &Mutex<HeadlessBackend>,
    config: &Config,
) -> Result<(), String> {
    ensure_dir(&config.state_dir)?;
    let history = config.state_dir.join("history");
    ensure_dir(&history)?;

    let snap = {
        let s = store.read().unwrap();
        Snapshot {
            version: SNAPSHOT_VERSION,
            saved_at: Utc::now(),
            workspaces: s.workspaces.values().cloned().collect(),
            tabs: s.tabs.values().cloned().collect(),
            panes: s.panes.values().cloned().collect(),
            notifications: s.notifications.iter().cloned().collect(),
        }
    };

    // Tails first (best-effort context, raw lines, byte-capped).
    let live_ids: HashSet<String> = snap.panes.iter().map(|p| p.id.clone()).collect();
    {
        let terms = terms.lock().unwrap();
        for pane in &snap.panes {
            let path = tail_path(&history, &pane.id);
            if let Some(lines) = terms.tail(&pane.id, usize::MAX, false) {
                let mut bytes = 0usize;
                let mut kept: Vec<&str> = Vec::new();
                for line in lines.iter().rev() {
                    bytes += line.len() + 1;
                    if bytes > config.history_tail_bytes {
                        break;
                    }
                    kept.push(line);
                }
                kept.reverse();
                let content = kept.join("\n");
                atomic_write(&path, content.as_bytes())?;
            }
        }
    }
    // Prune tails for panes that no longer exist.
    if let Ok(entries) = fs::read_dir(&history) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".tail") {
                if !live_ids.contains(id) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }

    let data = serde_json::to_vec_pretty(&snap).map_err(|e| e.to_string())?;
    atomic_write(&config.state_dir.join("snapshot.json"), &data)?;
    Ok(())
}

pub struct LoadedSnapshot {
    pub snapshot: Snapshot,
    pub tails: HashMap<String, Vec<String>>,
}

pub fn load(config: &Config) -> Option<LoadedSnapshot> {
    let path = config.state_dir.join("snapshot.json");
    let data = fs::read(&path).ok()?;
    let snapshot: Snapshot = match serde_json::from_slice(&data) {
        Ok(s) => s,
        Err(e) => {
            // Back up corrupt snapshot, start empty, log loudly.
            let backup = config.state_dir.join(format!(
                "snapshot.corrupt-{}",
                Utc::now().format("%Y%m%dT%H%M%S")
            ));
            let _ = fs::rename(&path, &backup);
            tracing::warn!(
                "corrupt snapshot ({}), backed up to {:?}; starting empty",
                e,
                backup
            );
            return None;
        }
    };
    if snapshot.version != SNAPSHOT_VERSION {
        tracing::error!(
            "snapshot version {} != {}; refusing to load",
            snapshot.version,
            SNAPSHOT_VERSION
        );
        return None;
    }
    let history = config.state_dir.join("history");
    let mut tails = HashMap::new();
    for pane in &snapshot.panes {
        let path = tail_path(&history, &pane.id);
        if let Ok(content) = fs::read_to_string(&path) {
            tails.insert(
                pane.id.clone(),
                content.lines().map(|l| l.to_string()).collect(),
            );
        }
    }
    Some(LoadedSnapshot { snapshot, tails })
}

/// Rebuild store + headless surfaces from a snapshot. No processes are
/// started here — ever. Panes become RESTORED/RESUMABLE/EXITED.
pub fn apply(store: &SharedStore, terms: &Mutex<HeadlessBackend>, mut loaded: LoadedSnapshot) {
    let mut s = store.write().unwrap();
    // Legacy snapshots predate handles: backfill unique ones. Sorted by id
    // so the `-2` suffixes land deterministically; each insert keeps the
    // collision check honest for the next legacy workspace.
    loaded.snapshot.workspaces.sort_by(|a, b| a.id.cmp(&b.id));
    for ws in loaded.snapshot.workspaces {
        let mut ws = ws;
        if ws.handle.is_empty() {
            ws.handle = crate::router::unique_handle(&s, &signaltty_core::model::slugify(&ws.name));
        }
        s.workspaces.insert(ws.id.clone(), ws);
    }
    for tab in loaded.snapshot.tabs {
        s.tabs.insert(tab.id.clone(), tab);
    }
    let mut terms = terms.lock().unwrap();
    for mut pane in loaded.snapshot.panes {
        let was_live = matches!(pane.live, LiveState::Live);
        if was_live {
            // The process is gone with the old server; keep the code
            // unknown rather than inventing one.
            pane.live = LiveState::Exited { code: None };
        }
        pane.restore_state = if !was_live {
            RestoreState::Exited
        } else if pane.agent.resume_argv.is_some() {
            RestoreState::Resumable
        } else {
            RestoreState::Restored
        };
        if let Some(lines) = loaded.tails.remove(&pane.id) {
            terms.restore_surface(&pane.id, pane.pty_size.cols, pane.pty_size.rows, lines);
        } else {
            terms.create_surface(&pane.id, pane.pty_size.cols, pane.pty_size.rows);
        }
        s.panes.insert(pane.id.clone(), pane);
    }
    for n in loaded.snapshot.notifications {
        s.push_notification(n);
    }
}

fn tail_path(history: &Path, pane_id: &str) -> PathBuf {
    let safe: String = pane_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    history.join(format!("{safe}.tail"))
}

fn ensure_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}
