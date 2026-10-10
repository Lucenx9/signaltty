//! Live `/proc` refresh (docs/07 layer 4): the pane's spawn argv is only
//! the first word. Every 10s the tick below re-reads the deepest live
//! descendant (shell-transparent) and the child's cwd, promoting the
//! agent kind and following directory changes. Emits `pane.updated`
//! per changed pane — quiet when nothing changed, so no event spam and
//! no persistence churn. Linux-only; elsewhere the tick is a no-op.

use std::collections::HashMap;

use chrono::Utc;

use signaltty_agent::{AgentAdapter, OverlayAdapter, ProcessInfo};
use signaltty_core::model::{AgentKind, LiveState};

use crate::store::{SharedStore, StoredEvent};

/// Basenames treated as wrappers: look past them to the real agent.
const SHELLS: &[&str] = &["sh", "bash", "zsh", "fish", "dash"];

fn read_stat(pid: u32) -> Option<(u32, String)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // comm is `(name)` and may contain spaces/parens: split after the last ')'.
    let after = stat.rsplit(')').next()?;
    let mut fields = after.split_whitespace();
    let _state = fields.next()?;
    let ppid: u32 = fields.next()?.parse().ok()?;
    let (before, _) = stat.rsplit_once(')')?;
    let (_, comm) = before.split_once('(')?;
    Some((ppid, comm.to_string()))
}

fn children_of(ppid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return out;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(pid): Option<u32> = name.to_str().and_then(|s| s.parse().ok()) else {
            continue;
        };
        if let Some((parent, _)) = read_stat(pid) {
            if parent == ppid {
                out.push(pid);
            }
        }
    }
    out.sort_unstable();
    out
}

/// Youngest deepest descendant of `pid`: follow the newest child while
/// the current basename is a shell, then stop. One shell-transparent
/// level keeps this cheap and honest (not a process-tree oracle).
pub fn deepest_descendant(pid: u32) -> u32 {
    let mut current = pid;
    for _ in 0..8 {
        let argv0 = proc_argv0(current).unwrap_or_default();
        if !SHELLS.contains(&argv0.as_str()) {
            break;
        }
        match children_of(current).into_iter().max() {
            Some(child) => current = child,
            None => break,
        }
    }
    current
}

/// Basename of `/proc/<pid>/cmdline`'s first field.
pub fn proc_argv0(pid: u32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let first = raw.split(|b| *b == 0).next()?.to_vec();
    if first.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&first).to_string();
    Some(text.rsplit('/').next().unwrap_or(&text).to_string())
}

/// All of `/proc/<pid>/cmdline`'s fields.
pub fn proc_argv(pid: u32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|f| !f.is_empty())
        .map(|f| String::from_utf8_lossy(f).to_string())
        .collect();
    (!argv.is_empty()).then_some(argv)
}

/// Target of `/proc/<pid>/cwd` (None when unreadable or gone).
pub fn proc_cwd(pid: u32) -> Option<String> {
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .map(|p| p.to_string_lossy().to_string())
}

/// One tick: promote kinds and follow cwds. Returns the events to
/// broadcast (empty when nothing changed).
pub fn scan(
    store: &SharedStore,
    child_pids: &HashMap<String, u32>,
    overlays: &[OverlayAdapter],
) -> Vec<StoredEvent> {
    // Snapshot the live set first so the /proc walk runs lock-free.
    let live: Vec<(String, AgentKind, String)> = {
        let s = store.read().unwrap();
        s.panes
            .values()
            .filter(|p| matches!(p.live, LiveState::Live))
            .map(|p| (p.id.clone(), p.agent.kind, p.cwd.clone()))
            .collect()
    };
    let mut changed: Vec<(String, Option<AgentKind>, Option<String>)> = Vec::new();
    let mut names: Vec<(String, String)> = Vec::new();
    for (pane_id, kind, cwd) in live {
        let Some(child) = child_pids.get(&pane_id).copied() else {
            continue;
        };
        let target = deepest_descendant(child);
        if let Some(argv) = proc_argv(target) {
            names.push((
                pane_id.clone(),
                signaltty_agent::process::process_name(&argv),
            ));
        }
        let bin = proc_argv0(target).unwrap_or_default();
        let new_kind = if matches!(kind, AgentKind::Generic | AgentKind::None) && !bin.is_empty() {
            let proc = ProcessInfo {
                argv: vec![bin],
                cwd: String::new(),
                pid: Some(target),
            };
            let mut promoted = None;
            for overlay in overlays {
                if matches!(overlay.kind(), AgentKind::Generic | AgentKind::None) {
                    continue;
                }
                if overlay.identify(&proc) {
                    promoted = Some(overlay.kind());
                    break;
                }
            }
            promoted.or_else(|| {
                let specific = signaltty_agent::detect_kind(&proc.argv);
                if matches!(specific, AgentKind::Generic) {
                    None
                } else {
                    Some(specific)
                }
            })
        } else {
            None
        };
        let new_cwd = proc_cwd(target).filter(|c| c != &cwd);
        if new_kind.is_some() || new_cwd.is_some() {
            changed.push((pane_id, new_kind, new_cwd));
        }
    }
    let mut s = store.write().unwrap();
    // Screen-rule scoping state (spec 033): no event, no persist.
    for (pane_id, name) in names {
        s.set_process_name(&pane_id, name);
    }
    if changed.is_empty() {
        return Vec::new();
    }
    let mut outbound = Vec::new();
    for (pane_id, kind, cwd) in changed {
        let Some(pane) = s.panes.get_mut(&pane_id) else {
            continue;
        };
        // The child may have exited between the walk and the lock.
        if !matches!(pane.live, LiveState::Live) {
            continue;
        }
        if let Some(kind) = kind {
            // Promotion only: never demote a hooked/known agent.
            if matches!(pane.agent.kind, AgentKind::Generic | AgentKind::None) {
                pane.agent.kind = kind;
            }
        }
        if let Some(cwd) = cwd {
            if std::path::Path::new(&cwd).is_dir() {
                pane.cwd = cwd;
            }
        }
        pane.last_activity_at = Utc::now();
        outbound.push(s.emit(
            signaltty_proto::event::PANE_UPDATED,
            serde_json::json!({"pane_id": pane_id}),
        ));
    }
    outbound
}

#[cfg(test)]
mod tests {
    use super::*;

    fn self_pid() -> u32 {
        std::process::id()
    }

    #[test]
    fn own_process_reads_back() {
        let pid = self_pid();
        let argv0 = proc_argv0(pid).unwrap();
        assert!(!argv0.is_empty(), "test binary has argv[0]");
        let cwd = proc_cwd(pid).unwrap();
        assert!(std::path::Path::new(&cwd).is_dir());
        // read_stat agrees on parentage with std and extracts comm.
        let (ppid, comm) = read_stat(pid).unwrap();
        assert!(ppid > 0);
        let expected = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap();
        assert_eq!(
            comm,
            expected.trim_end_matches('\n'),
            "comm matches /proc/<pid>/comm"
        );
    }

    #[test]
    fn unknown_pids_read_none() {
        assert_eq!(proc_argv0(1 << 30), None);
        assert_eq!(proc_cwd(1 << 30), None);
        assert_eq!(read_stat(1 << 30), None);
    }

    #[test]
    fn deepest_descendant_stops_at_non_shell() {
        // The test binary is not a shell: descent stops immediately.
        assert_eq!(deepest_descendant(self_pid()), self_pid());
    }
}
