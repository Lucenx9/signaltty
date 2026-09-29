//! Event invalidation and workspace snapshots, independent of GTK.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{json, Value};
use signaltty_core::{Pane, Tab, Workspace};
use signaltty_proto::event;

#[derive(Clone, Deserialize)]
pub struct Snapshot {
    pub workspace: Workspace,
    pub tabs: Vec<Tab>,
    pub panes: Vec<Pane>,
}

#[derive(Default)]
pub struct WorkspaceCache {
    pub workspaces: Vec<Workspace>,
    pub snapshots: HashMap<String, Snapshot>,
    pane_workspaces: HashMap<String, String>,
    tab_workspaces: HashMap<String, String>,
}

#[derive(Default)]
pub struct PendingRefresh {
    pub full: bool,
    pub workspaces: HashSet<String>,
    unknown_panes: HashSet<String>,
}

impl PendingRefresh {
    pub fn full() -> Self {
        Self {
            full: true,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full && self.workspaces.is_empty() && self.unknown_panes.is_empty()
    }

    pub fn on_event(&mut self, cache: &WorkspaceCache, name: &str, payload: &Value) {
        match name {
            event::WORKSPACE_CREATED | event::WORKSPACE_CLOSED => {
                self.full = true;
                return;
            }
            event::WORKSPACE_UPDATED
            | event::GIT_BRANCH_CHANGED
            | event::TAB_CREATED
            | event::TAB_UPDATED
            | event::TAB_CLOSED
            | event::PANE_CREATED
            | event::PANE_UPDATED
            | event::PANE_CLOSED
            | event::PANE_EXITED
            | event::PANE_RESIZED
            | event::AGENT_WORKING
            | event::AGENT_BLOCKED
            | event::AGENT_DONE
            | event::AGENT_FAILED
            | event::AGENT_IDLE
            | event::AGENT_UNKNOWN
            | event::AGENT_EXITED
            | event::ATTENTION_CREATED
            | event::ATTENTION_UPDATED
            | event::ATTENTION_CLEARED
            | event::NOTIFICATION_CREATED => {}
            // PTY output is streamed directly; shutdown and future unrelated
            // events do not invalidate workspace data.
            _ => return,
        }
        if self.full {
            return;
        }
        let workspace = payload["workspace_id"]
            .as_str()
            .or_else(|| payload["workspace"]["id"].as_str())
            .or_else(|| payload["pane"]["workspace_id"].as_str())
            .or_else(|| payload["tab"]["workspace_id"].as_str())
            .or_else(|| payload["notification"]["workspace_id"].as_str());
        if let Some(id) = workspace {
            self.workspaces.insert(id.to_string());
        } else if let Some(id) = payload["pane_id"]
            .as_str()
            .or_else(|| payload["notification"]["pane_id"].as_str())
        {
            if let Some(ws) = cache.pane_workspaces.get(id) {
                self.workspaces.insert(ws.clone());
            } else {
                // Resolve once per batch, never once per event. A pane that
                // has already disappeared cannot affect a cached workspace.
                self.unknown_panes.insert(id.to_string());
            }
        } else if let Some(ws) = payload["tab_id"]
            .as_str()
            .and_then(|id| cache.tab_workspaces.get(id))
        {
            self.workspaces.insert(ws.clone());
        }
        // A newly created then closed tab may not be cached yet. Its
        // tab.created event already invalidated the owning workspace.
    }
}

impl WorkspaceCache {
    fn remove(&mut self, id: &str) {
        if let Some(old) = self.snapshots.remove(id) {
            for pane in old.panes {
                self.pane_workspaces.remove(&pane.id);
            }
            for tab in old.tabs {
                self.tab_workspaces.remove(&tab.id);
            }
        }
    }

    fn replace(&mut self, snapshot: Snapshot) {
        let id = &snapshot.workspace.id;
        self.remove(id);
        for pane in &snapshot.panes {
            self.pane_workspaces.insert(pane.id.clone(), id.clone());
        }
        for tab in &snapshot.tabs {
            self.tab_workspaces.insert(tab.id.clone(), id.clone());
        }
        match self.workspaces.iter_mut().find(|ws| ws.id == *id) {
            Some(ws) => *ws = snapshot.workspace.clone(),
            None => self.workspaces.push(snapshot.workspace.clone()),
        }
        self.snapshots.insert(id.clone(), snapshot);
    }

    /// Returns successful workspace replacements and errors. Failed reads keep
    /// the last good snapshot; only an authoritative list removes workspaces.
    pub fn refresh(
        &mut self,
        mut pending: PendingRefresh,
        mut call: impl FnMut(&str, Value) -> Result<Value, String>,
    ) -> (HashSet<String>, Vec<String>) {
        let mut errors = Vec::new();
        if pending.full {
            let list = call("workspace.list", json!({})).and_then(|v| {
                serde_json::from_value::<Vec<Workspace>>(v["workspaces"].clone())
                    .map_err(|e| e.to_string())
            });
            let list = match list {
                Ok(list) => list,
                Err(e) => return (HashSet::new(), vec![e]),
            };
            pending.workspaces = list.iter().map(|ws| ws.id.clone()).collect();
            let removed: Vec<_> = self
                .snapshots
                .keys()
                .filter(|id| !pending.workspaces.contains(*id))
                .cloned()
                .collect();
            for id in removed {
                self.remove(&id);
            }
            self.workspaces = list;
        } else {
            for id in pending.unknown_panes {
                if let Ok(v) = call("pane.get", json!({"pane_id": id})) {
                    if let Some(ws) = v["pane"]["workspace_id"].as_str() {
                        pending.workspaces.insert(ws.to_string());
                    }
                }
            }
        }
        let mut changed = HashSet::new();
        for id in pending.workspaces {
            let snapshot = call("workspace.get", json!({"workspace_id": id}))
                .and_then(|v| serde_json::from_value::<Snapshot>(v).map_err(|e| e.to_string()));
            match snapshot {
                Ok(snapshot) => {
                    self.replace(snapshot);
                    changed.insert(id);
                }
                Err(e) => errors.push(e),
            }
        }
        (changed, errors)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn fixture(id: &str) -> Value {
        let now = "2026-09-28T12:00:00Z";
        let pane = format!("pane_{id}");
        let tab = format!("tab_{id}");
        json!({
            "workspace": {"id": id, "name": id, "cwd": "/tmp", "tabs": [tab],
                "active_tab_id": tab, "created_at": now, "updated_at": now},
            "tabs": [{"id": tab, "workspace_id": id, "title": tab,
                "layout": {"type": "pane", "pane_id": pane},
                "active_pane_id": pane, "created_at": now}],
            "panes": [{"id": pane, "workspace_id": id, "tab_id": tab,
                "title": pane, "cwd": "/tmp", "argv": ["sh"],
                "pty_size": {"cols": 80, "rows": 24}, "live": {"state": "live"},
                "restore_state": "LIVE", "agent": {"kind": "none"},
                "lifecycle": "idle", "last_lifecycle": "unknown", "attention": "none",
                "created_at": now, "last_activity_at": now}]
        })
    }

    fn cache() -> WorkspaceCache {
        let mut cache = WorkspaceCache::default();
        for id in ["a", "b"] {
            cache.replace(serde_json::from_value(fixture(id)).unwrap());
        }
        cache
    }

    #[test]
    fn burst_reads_changed_workspaces_once_and_preserves_the_others() {
        let mut cache = cache();
        let mut pending = PendingRefresh::default();
        for _ in 0..200 {
            pending.on_event(&cache, event::AGENT_WORKING, &json!({"pane_id": "pane_b"}));
        }
        let mut calls = Vec::new();
        let (changed, errors) = cache.refresh(pending, |method, params| {
            calls.push((method.to_string(), params));
            let mut snapshot = fixture("b");
            snapshot["panes"][0]["lifecycle"] = json!("working");
            Ok(snapshot)
        });
        assert!(errors.is_empty());
        assert_eq!(
            calls,
            vec![("workspace.get".into(), json!({"workspace_id": "b"}))]
        );
        assert_eq!(changed, HashSet::from(["b".into()]));
        assert_eq!(
            cache.snapshots["b"].panes[0].lifecycle,
            signaltty_core::Lifecycle::Working
        );
        assert_eq!(
            cache.snapshots["a"].panes[0].lifecycle,
            signaltty_core::Lifecycle::Idle
        );
    }

    #[test]
    fn routes_all_workspace_payload_shapes_including_inactive_deletions() {
        let cache = cache();
        let events = [
            (event::WORKSPACE_UPDATED, json!({"workspace": {"id": "b"}})),
            (event::GIT_BRANCH_CHANGED, json!({"workspace_id": "b"})),
            (event::TAB_CREATED, json!({"tab": {"workspace_id": "b"}})),
            (event::TAB_UPDATED, json!({"tab": {"workspace_id": "b"}})),
            (event::TAB_CLOSED, json!({"tab_id": "tab_b"})),
            (event::PANE_CREATED, json!({"pane": {"workspace_id": "b"}})),
            (event::PANE_UPDATED, json!({"pane": {"workspace_id": "b"}})),
            (event::PANE_RESIZED, json!({"pane": {"workspace_id": "b"}})),
            (event::PANE_RESIZED, json!({"pane_id": "pane_b"})),
            (event::PANE_CLOSED, json!({"pane_id": "pane_b"})),
            (event::PANE_EXITED, json!({"pane_id": "pane_b"})),
            (event::AGENT_WORKING, json!({"pane_id": "pane_b"})),
            (event::AGENT_BLOCKED, json!({"pane_id": "pane_b"})),
            (event::AGENT_DONE, json!({"pane_id": "pane_b"})),
            (event::AGENT_FAILED, json!({"pane_id": "pane_b"})),
            (event::AGENT_IDLE, json!({"pane_id": "pane_b"})),
            (event::AGENT_UNKNOWN, json!({"pane_id": "pane_b"})),
            (event::AGENT_EXITED, json!({"pane_id": "pane_b"})),
            (event::ATTENTION_CREATED, json!({"pane_id": "pane_b"})),
            (event::ATTENTION_UPDATED, json!({"pane_id": "pane_b"})),
            (event::ATTENTION_CLEARED, json!({"pane_id": "pane_b"})),
            (
                event::NOTIFICATION_CREATED,
                json!({"notification": {"workspace_id": "b", "pane_id": "pane_b"}}),
            ),
            (
                event::NOTIFICATION_CREATED,
                json!({"notification": {"pane_id": "pane_b"}}),
            ),
        ];
        for (name, payload) in events {
            let mut pending = PendingRefresh::default();
            pending.on_event(&cache, name, &payload);
            assert!(!pending.full, "{name}");
            assert!(pending.unknown_panes.is_empty(), "{name}");
            assert_eq!(pending.workspaces, HashSet::from(["b".into()]), "{name}");
        }
    }

    #[test]
    fn batch_unions_workspaces_and_resolves_unknown_panes_only_once() {
        let mut cache = cache();
        let mut pending = PendingRefresh::default();
        for _ in 0..200 {
            pending.on_event(&cache, event::AGENT_WORKING, &json!({"pane_id": "new"}));
            pending.on_event(
                &cache,
                event::ATTENTION_CLEARED,
                &json!({"pane_id": "pane_a"}),
            );
        }
        let mut calls = Vec::new();
        cache.refresh(pending, |method, params| {
            calls.push(method.to_string());
            Ok(if method == "pane.get" {
                json!({"pane": {"workspace_id": "b"}})
            } else {
                fixture(params["workspace_id"].as_str().unwrap())
            })
        });
        assert_eq!(calls, ["pane.get", "workspace.get", "workspace.get"]);
    }

    #[test]
    fn full_batch_supersedes_partial_work_and_removes_stale_indexes() {
        for event in [event::WORKSPACE_CREATED, event::WORKSPACE_CLOSED] {
            let mut cache = cache();
            let mut pending = PendingRefresh::default();
            pending.on_event(&cache, event::AGENT_WORKING, &json!({"pane_id": "unknown"}));
            pending.on_event(&cache, event, &json!({}));
            let mut calls = Vec::new();
            cache.refresh(pending, |method, _| {
                calls.push(method.to_string());
                Ok(if method == "workspace.list" {
                    json!({"workspaces": [fixture("a")["workspace"]]})
                } else {
                    fixture("a")
                })
            });
            assert_eq!(calls, ["workspace.list", "workspace.get"]);
            assert_eq!(cache.workspaces.len(), 1);
            assert!(!cache.snapshots.contains_key("b"));
            assert!(!cache.pane_workspaces.contains_key("pane_b"));
            assert!(!cache.tab_workspaces.contains_key("tab_b"));
        }
    }

    #[test]
    fn create_then_close_in_one_frame_still_refreshes_the_owner() {
        let mut cache = cache();
        let mut pending = PendingRefresh::default();
        pending.on_event(
            &cache,
            event::TAB_CREATED,
            &json!({"tab": {"id": "new", "workspace_id": "b"}}),
        );
        pending.on_event(&cache, event::TAB_CLOSED, &json!({"tab_id": "new"}));
        pending.on_event(&cache, event::PANE_CLOSED, &json!({"pane_id": "pane_b"}));
        cache.refresh(pending, |method, params| {
            assert_eq!(method, "workspace.get");
            assert_eq!(params, json!({"workspace_id": "b"}));
            let mut snapshot = fixture("b");
            snapshot["tabs"] = json!([]);
            snapshot["panes"] = json!([]);
            snapshot["workspace"]["tabs"] = json!([]);
            Ok(snapshot)
        });
        assert!(!cache.pane_workspaces.contains_key("pane_b"));
        assert!(!cache.tab_workspaces.contains_key("tab_b"));
    }

    #[test]
    fn failed_reads_preserve_cached_state_and_retry_on_next_event() {
        let mut cache = cache();
        let (_, errors) = cache.refresh(PendingRefresh::full(), |_, _| Err("eof".into()));
        assert_eq!(errors, ["eof"]);
        assert_eq!(cache.workspaces.len(), 2);
        let mut pending = PendingRefresh::default();
        pending.on_event(&cache, event::PANE_UPDATED, &json!({"pane_id": "pane_b"}));
        let (_, errors) = cache.refresh(pending, |_, _| Err("eof".into()));
        assert_eq!(errors, ["eof"]);
        assert_eq!(cache.snapshots.len(), 2);
        let mut pending = PendingRefresh::default();
        pending.on_event(&cache, event::PANE_UPDATED, &json!({"pane_id": "pane_b"}));
        assert_eq!(pending.workspaces, HashSet::from(["b".into()]));
    }

    #[test]
    fn unrelated_or_unroutable_events_do_not_read_workspaces() {
        let cache = cache();
        let mut pending = PendingRefresh::default();
        for name in [
            event::PTY_DATA,
            "pty.snapshot",
            event::SERVER_WILL_SHUTDOWN,
            "future.event",
        ] {
            pending.on_event(&cache, name, &json!({"pane_id": "pane_b"}));
        }
        pending.on_event(
            &cache,
            event::NOTIFICATION_CREATED,
            &json!({"notification": {"pane_id": null}}),
        );
        assert!(pending.is_empty());
    }
}
