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

use signaltty_core::model::{LiveState, Notification, Pane, RestoreState, Tab, Task, Workspace};
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
    #[serde(default)]
    pub tasks: Vec<Task>,
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
            tasks: s.tasks.values().cloned().collect(),
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

    if let Err(e) = rotate_snapshot_history(&config.state_dir) {
        tracing::warn!("snapshot history rotation failed: {e}");
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
        if let Some(decision) = &mut pane.pending_decision {
            if crate::approvals::Approvals::is_native(&decision.id) {
                decision.answerable = false;
            }
        }
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
    // Read cursors are runtime-only: `restore_surface` already reset them
    // so pre-restart cursors read as dropped (headless `dropped_floor`).
    for task in loaded.snapshot.tasks {
        s.tasks.insert(task.id.clone(), task);
    }
}

/// Recovery: no process survives a restart (docs/09), so every non-terminal
/// task's worker is dead. Fail them with evidence via the `task_fail`
/// transition (pairs mutate + emit per the AGENTS.md rule); completed work,
/// results, dispositions, and checkouts stay intact. Call after
/// `Store::configure_events` so the transitions are journaled and broadcast.
pub fn recover_tasks(store: &SharedStore) {
    let mut s = store.write().unwrap();
    let stale: Vec<String> = s
        .tasks
        .values()
        .filter(|t| !t.state.is_terminal())
        .map(|t| t.id.clone())
        .collect();
    for id in stale {
        s.task_fail(&id, Some(serde_json::json!({"stage": "restart"})));
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

const HISTORY_KEEP: usize = 48;
const HISTORY_INTERVAL_SECS: i64 = 15 * 60;

fn history_stamp(name: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(name, "snapshot-%Y%m%dT%H%M%SZ.json")
        .ok()
        .map(|t| t.and_utc())
}

/// Copy the current `snapshot.json` into `snapshots/` before it is replaced:
/// at most one copy per 15 minutes, never an empty session, newest 48 kept.
fn rotate_snapshot_history(state_dir: &Path) -> Result<(), String> {
    let current = state_dir.join("snapshot.json");
    let dir = state_dir.join("snapshots");
    let mut names: Vec<String> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| history_stamp(n).is_some())
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };
    names.sort();
    // Names are `snapshot-<UTC %Y%m%dT%H%M%SZ>.json`, so they sort by time.
    // A future stamp (clock moved back, copied state dir) is not recent.
    let recent = names
        .last()
        .and_then(|newest| history_stamp(newest))
        .is_some_and(|t| (0..HISTORY_INTERVAL_SECS).contains(&(Utc::now() - t).num_seconds()));
    if recent {
        return Ok(());
    }
    let data = match fs::read(&current) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{}: {e}", current.display())),
    };
    if !serde_json::from_slice::<Snapshot>(&data).is_ok_and(|s| !s.workspaces.is_empty()) {
        return Ok(());
    }
    ensure_dir(&dir)?;
    let name = format!("snapshot-{}.json", Utc::now().format("%Y%m%dT%H%M%SZ"));
    atomic_write(&dir.join(&name), &data)?;
    names.push(name);
    for old in &names[..names.len().saturating_sub(HISTORY_KEEP)] {
        let _ = fs::remove_file(dir.join(old));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state_dir() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(signaltty_core::ids::new_pane_id())
            .join("state")
    }

    fn make_snapshot_json(workspaces: &[&str]) -> String {
        let ws_objs: Vec<serde_json::Value> = workspaces
            .iter()
            .map(|id| {
                serde_json::json!({
                    "id": id,
                    "name": id,
                    "cwd": "/tmp",
                    "tabs": [],
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                })
            })
            .collect();
        serde_json::json!({
            "version": 1,
            "saved_at": "2026-01-01T00:00:00Z",
            "workspaces": ws_objs,
            "tabs": [],
            "panes": [],
            "notifications": [],
            "tasks": [],
        })
        .to_string()
    }

    #[test]
    fn preserves_on_first_save() {
        let state_dir = test_state_dir();
        fs::create_dir_all(&state_dir).unwrap();
        let snap_path = state_dir.join("snapshot.json");
        fs::write(&snap_path, make_snapshot_json(&["ws-1"])).unwrap();

        rotate_snapshot_history(&state_dir).unwrap();

        let snapshots_dir = state_dir.join("snapshots");
        let entries: Vec<_> = fs::read_dir(&snapshots_dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(name.starts_with("snapshot-") && name.ends_with(".json"));
        let perms = entry.metadata().unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
        let content = fs::read_to_string(entry.path()).unwrap();
        assert!(content.contains("ws-1"));
    }

    #[test]
    fn skips_within_15_minutes() {
        let state_dir = test_state_dir();
        let snapshots_dir = state_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::write(
            state_dir.join("snapshot.json"),
            make_snapshot_json(&["ws-1"]),
        )
        .unwrap();
        let stamp = |mins: i64| {
            let t = Utc::now() - chrono::Duration::minutes(mins);
            format!("snapshot-{}.json", t.format("%Y%m%dT%H%M%SZ"))
        };

        fs::write(snapshots_dir.join(stamp(10)), "{}").unwrap();
        rotate_snapshot_history(&state_dir).unwrap();
        assert_eq!(fs::read_dir(&snapshots_dir).unwrap().count(), 1);

        fs::remove_dir_all(&snapshots_dir).unwrap();
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::write(snapshots_dir.join(stamp(20)), "{}").unwrap();
        rotate_snapshot_history(&state_dir).unwrap();
        assert_eq!(fs::read_dir(&snapshots_dir).unwrap().count(), 2);
    }

    #[test]
    fn future_and_stray_names_do_not_block_history() {
        let state_dir = test_state_dir();
        let snapshots_dir = state_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).unwrap();
        fs::write(
            state_dir.join("snapshot.json"),
            make_snapshot_json(&["ws-1"]),
        )
        .unwrap();
        let future = Utc::now() + chrono::Duration::hours(1);
        let future = format!("snapshot-{}.json", future.format("%Y%m%dT%H%M%SZ"));
        fs::write(snapshots_dir.join(&future), "{}").unwrap();
        fs::write(snapshots_dir.join("snapshot-zzz.json"), "{}").unwrap();

        rotate_snapshot_history(&state_dir).unwrap();

        assert_eq!(fs::read_dir(&snapshots_dir).unwrap().count(), 3);

        // A stray name sorting after a recent copy must not defeat the gate.
        fs::remove_dir_all(&snapshots_dir).unwrap();
        fs::create_dir_all(&snapshots_dir).unwrap();
        let recent = Utc::now() - chrono::Duration::minutes(5);
        let recent = format!("snapshot-{}.json", recent.format("%Y%m%dT%H%M%SZ"));
        fs::write(snapshots_dir.join(recent), "{}").unwrap();
        fs::write(snapshots_dir.join("snapshot-zzz.json"), "{}").unwrap();
        rotate_snapshot_history(&state_dir).unwrap();
        assert_eq!(fs::read_dir(&snapshots_dir).unwrap().count(), 2);
    }

    #[test]
    fn skips_empty_snapshots() {
        let state_dir = test_state_dir();
        fs::create_dir_all(&state_dir).unwrap();
        let snap_path = state_dir.join("snapshot.json");
        fs::write(&snap_path, make_snapshot_json(&[])).unwrap();

        rotate_snapshot_history(&state_dir).unwrap();

        assert!(!state_dir.join("snapshots").exists());
    }

    #[test]
    fn prunes_to_48() {
        let state_dir = test_state_dir();
        let snapshots_dir = state_dir.join("snapshots");
        fs::create_dir_all(&snapshots_dir).unwrap();

        // Populate with 48 older files
        for i in 0..48 {
            let name = format!("snapshot-20200101T00{:02}00Z.json", i);
            fs::write(snapshots_dir.join(name), "{}").unwrap();
        }

        let snap_path = state_dir.join("snapshot.json");
        fs::write(&snap_path, make_snapshot_json(&["ws-1"])).unwrap();

        rotate_snapshot_history(&state_dir).unwrap();

        let mut remaining: Vec<String> = fs::read_dir(&snapshots_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        remaining.sort();

        assert_eq!(remaining.len(), 48);
        // Oldest file pruned
        assert!(!remaining.contains(&"snapshot-20200101T000000Z.json".to_string()));
        // Second oldest is kept
        assert!(remaining.contains(&"snapshot-20200101T000100Z.json".to_string()));
        // Newest file added
        assert!(remaining.last().unwrap().starts_with("snapshot-"));
    }
}
