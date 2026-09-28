//! In-memory authoritative store. Uses std RwLock so both tokio tasks
//! and PTY pump threads can share it. Hold locks briefly, never across await.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde_json::Value;

use signaltty_core::model::{Notification, Pane, Tab, Workspace};
use signaltty_core::state::{Attention, Lifecycle};

pub const MAX_NOTIFICATIONS: usize = 200;
pub const MAX_EVENTS: usize = 1024;

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub seq: u64,
    pub name: String,
    pub payload: Value,
}

pub struct Store {
    pub workspaces: HashMap<String, Workspace>,
    pub tabs: HashMap<String, Tab>,
    pub panes: HashMap<String, Pane>,
    pub notifications: VecDeque<Notification>,
    pub events: VecDeque<StoredEvent>,
    pub seq: u64,
    pub started_at: DateTime<Utc>,
}

impl Store {
    pub fn new() -> Store {
        Store {
            workspaces: HashMap::new(),
            tabs: HashMap::new(),
            panes: HashMap::new(),
            notifications: VecDeque::new(),
            events: VecDeque::new(),
            seq: 0,
            started_at: Utc::now(),
        }
    }

    /// Assign seq, append to replay ring, return for broadcast.
    pub fn emit(&mut self, name: &str, payload: Value) -> StoredEvent {
        self.seq += 1;
        let ev = StoredEvent {
            seq: self.seq,
            name: name.to_string(),
            payload,
        };
        self.events.push_back(ev.clone());
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
        ev
    }

    pub fn push_notification(&mut self, n: Notification) {
        self.notifications.push_back(n);
        while self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications.pop_front();
        }
    }

    /// Bump the seq counter without recording in the replay ring.
    /// Used for high-volume `pty.data`, which live viewers get and
    /// reattach covers via snapshot.
    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    pub fn live_panes(&self) -> usize {
        use signaltty_core::model::LiveState;
        self.panes
            .values()
            .filter(|p| matches!(p.live, LiveState::Live))
            .count()
    }

    pub fn events_since(&self, from_seq: u64) -> Vec<StoredEvent> {
        self.events
            .iter()
            .filter(|e| e.seq > from_seq)
            .cloned()
            .collect()
    }

    /// Next pane needing a human: severity desc, then most recent activity.
    pub fn next_unread(&self) -> Option<String> {
        let mut panes: Vec<&Pane> = self
            .panes
            .values()
            .filter(|p| p.attention.needs_human())
            .collect();
        panes.sort_by(|a, b| {
            b.attention
                .severity()
                .cmp(&a.attention.severity())
                .then(b.last_activity_at.cmp(&a.last_activity_at))
        });
        panes.first().map(|p| p.id.clone())
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedStore = Arc<RwLock<Store>>;

/// Set lifecycle if changed; returns the event to broadcast, if any.
pub fn set_lifecycle(store: &SharedStore, pane_id: &str, next: Lifecycle) -> Option<StoredEvent> {
    let mut s = store.write().unwrap();
    let pane = s.panes.get_mut(pane_id)?;
    let prev = pane.lifecycle;
    if prev == next {
        return None;
    }
    pane.set_lifecycle(next);
    pane.last_activity_at = Utc::now();
    let ev = s.emit(
        next.event_name(),
        serde_json::json!({"pane_id": pane_id, "lifecycle": next.as_str(), "prev": prev.as_str()}),
    );
    Some(ev)
}

/// Raise attention if the new state is more severe; returns event if changed.
pub fn raise_attention(store: &SharedStore, pane_id: &str, next: Attention) -> Option<StoredEvent> {
    let mut s = store.write().unwrap();
    let pane = s.panes.get_mut(pane_id)?;
    let prev = pane.attention;
    let raised = prev.raise(next);
    if raised == prev {
        return None;
    }
    pane.attention = raised;
    pane.last_activity_at = Utc::now();
    let name = if prev == Attention::None {
        signaltty_proto::event::ATTENTION_CREATED
    } else {
        signaltty_proto::event::ATTENTION_UPDATED
    };
    let ev = s.emit(
        name,
        serde_json::json!({"pane_id": pane_id, "attention": raised.as_str(), "prev": prev.as_str()}),
    );
    Some(ev)
}

/// Clear attention explicitly (user replied / focused via hook). Returns
/// the event to broadcast, if anything changed.
pub fn clear_attention(store: &SharedStore, pane_id: &str, reason: &str) -> Option<StoredEvent> {
    let mut s = store.write().unwrap();
    let pane = s.panes.get_mut(pane_id)?;
    if pane.attention == Attention::None {
        return None;
    }
    pane.mark_seen(chrono::Utc::now());
    let ev = s.emit(
        signaltty_proto::event::ATTENTION_CLEARED,
        serde_json::json!({"pane_id": pane_id, "reason": reason}),
    );
    Some(ev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use signaltty_core::model::{LiveState, PtySize, RestoreState};
    use signaltty_core::{new_pane_id, AgentInfo};

    fn pane_with(att: Attention, mins_ago: i64) -> Pane {
        let now = Utc::now();
        Pane {
            id: new_pane_id(),
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            title: "t".into(),
            cwd: "/tmp".into(),
            argv: vec!["sh".into()],
            pty_size: PtySize::default(),
            live: LiveState::Live,
            restore_state: RestoreState::Live,
            agent: AgentInfo::default(),
            lifecycle: Lifecycle::Working,
            last_lifecycle: Lifecycle::Unknown,
            attention: att,
            last_message: None,
            created_at: now,
            last_activity_at: now - chrono::Duration::minutes(mins_ago),
            last_seen_at: None,
        }
    }

    #[test]
    fn next_unread_orders_by_severity_then_recency() {
        let mut store = Store::new();
        let unread_old = pane_with(Attention::Unread, 10);
        let unread_new = pane_with(Attention::Unread, 1);
        let err = pane_with(Attention::Error, 30);
        let none = pane_with(Attention::None, 0);
        let ids = (unread_old.id.clone(), unread_new.id.clone(), err.id.clone());
        for p in [unread_old, unread_new, err, none] {
            store.panes.insert(p.id.clone(), p);
        }
        // Error wins despite age.
        assert_eq!(store.next_unread(), Some(ids.2.clone()));
        store.panes.get_mut(&ids.2).unwrap().attention = Attention::None;
        // Then newest unread.
        assert_eq!(store.next_unread(), Some(ids.1));
        assert_ne!(store.next_unread(), Some(ids.0.clone()));
    }

    #[test]
    fn raise_attention_emits_created_then_updated() {
        let store: SharedStore = Arc::new(RwLock::new(Store::new()));
        let p = pane_with(Attention::None, 0);
        let id = p.id.clone();
        store.write().unwrap().panes.insert(id.clone(), p);
        let e1 = raise_attention(&store, &id, Attention::Unread).unwrap();
        assert_eq!(e1.name, "attention.created");
        assert!(raise_attention(&store, &id, Attention::Unread).is_none());
        let e2 = raise_attention(&store, &id, Attention::Error).unwrap();
        assert_eq!(e2.name, "attention.updated");
    }
}
