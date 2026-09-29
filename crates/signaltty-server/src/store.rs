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

impl Store {
    /// Set lifecycle if changed; returns the event to broadcast, if any.
    /// Callers hold the lock, so one event costs one lock cycle and the
    /// state change can never part from its emit.
    pub fn set_lifecycle(&mut self, pane_id: &str, next: Lifecycle) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        let prev = pane.lifecycle;
        if prev == next {
            return None;
        }
        let now = Utc::now();
        pane.set_lifecycle(next, now);
        pane.last_activity_at = now;
        let ev = self.emit(
            next.event_name(),
            serde_json::json!({"pane_id": pane_id, "lifecycle": next.as_str(), "prev": prev.as_str()}),
        );
        Some(ev)
    }

    /// Raise attention if the new state is more severe; returns event if changed.
    pub fn raise_attention(&mut self, pane_id: &str, next: Attention) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
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
        let ev = self.emit(
            name,
            serde_json::json!({"pane_id": pane_id, "attention": raised.as_str(), "prev": prev.as_str()}),
        );
        Some(ev)
    }

    /// Set (or supersede) the pane's pending decision. A supersede folds
    /// into a single `decision.created` carrying the previous id — one
    /// emit per transition, never a cleared+created pair.
    pub fn set_decision(
        &mut self,
        pane_id: &str,
        decision: signaltty_core::model::Decision,
    ) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        let prev = pane.pending_decision.replace(decision.clone());
        pane.last_activity_at = chrono::Utc::now();
        let ev = self.emit(
            signaltty_proto::event::DECISION_CREATED,
            serde_json::json!({
                "pane_id": pane_id, "decision": decision,
                "prev": prev.map(|d| d.id),
            }),
        );
        Some(ev)
    }

    /// Consume the pending decision on answer. `None` when absent or the
    /// id is stale (superseded / already answered): the caller reports
    /// `{answered: false}`, never an error.
    pub fn answer_decision(
        &mut self,
        pane_id: &str,
        decision_id: &str,
        option_id: &str,
    ) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        match &pane.pending_decision {
            Some(d) if d.id == decision_id => {}
            _ => return None,
        }
        pane.pending_decision = None;
        pane.last_activity_at = chrono::Utc::now();
        let ev = self.emit(
            signaltty_proto::event::DECISION_ANSWERED,
            serde_json::json!({
                "pane_id": pane_id,
                "decision_id": decision_id,
                "option_id": option_id,
            }),
        );
        Some(ev)
    }

    /// Drop the pending decision without answering. `None` when absent.
    pub fn clear_decision(&mut self, pane_id: &str, reason: &str) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        let dropped = pane.pending_decision.take()?;
        pane.last_activity_at = chrono::Utc::now();
        let ev = self.emit(
            signaltty_proto::event::DECISION_CLEARED,
            serde_json::json!({
                "pane_id": pane_id,
                "decision_id": dropped.id,
                "reason": reason,
            }),
        );
        Some(ev)
    }

    /// Clear attention explicitly (user replied / focused via hook). Returns
    /// the event to broadcast, if anything changed.
    pub fn clear_attention(&mut self, pane_id: &str, reason: &str) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        if pane.attention == Attention::None {
            return None;
        }
        pane.mark_seen(chrono::Utc::now());
        let ev = self.emit(
            signaltty_proto::event::ATTENTION_CLEARED,
            serde_json::json!({"pane_id": pane_id, "reason": reason}),
        );
        Some(ev)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signaltty_core::model::PtySize;

    fn pane_with(att: Attention, mins_ago: i64) -> Pane {
        let now = Utc::now();
        let mut p = Pane::new(
            "ws_1".into(),
            "tab_1".into(),
            "/tmp".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        p.lifecycle = Lifecycle::Working;
        p.attention = att;
        p.last_activity_at = now - chrono::Duration::minutes(mins_ago);
        p
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
        let mut store = Store::new();
        let p = pane_with(Attention::None, 0);
        let id = p.id.clone();
        store.panes.insert(id.clone(), p);
        let e1 = store.raise_attention(&id, Attention::Unread).unwrap();
        assert_eq!(e1.name, "attention.created");
        assert!(store.raise_attention(&id, Attention::Unread).is_none());
        let e2 = store.raise_attention(&id, Attention::Error).unwrap();
        assert_eq!(e2.name, "attention.updated");
    }

    #[test]
    fn set_lifecycle_emits_only_on_change() {
        let mut store = Store::new();
        let p = pane_with(Attention::None, 0);
        let id = p.id.clone();
        store.panes.insert(id.clone(), p);
        assert!(store.set_lifecycle(&id, Lifecycle::Working).is_none());
        let e = store.set_lifecycle(&id, Lifecycle::Done).unwrap();
        assert_eq!(e.name, "agent.done");
        assert!(store.set_lifecycle(&id, Lifecycle::Done).is_none());
    }

    #[test]
    fn decision_lifecycle_created_answered_cleared() {
        use signaltty_core::model::{Decision, DecisionOption};
        let mut store = Store::new();
        let p = pane_with(Attention::None, 0);
        let id = p.id.clone();
        store.panes.insert(id.clone(), p);
        let decision = |did: &str| Decision {
            id: did.to_string(),
            prompt: "Allow?".to_string(),
            options: vec![DecisionOption {
                id: "once".into(),
                label: "Once".into(),
            }],
            answerable: true,
            received_at: Utc::now(),
        };
        let e = store.set_decision(&id, decision("d1")).unwrap();
        assert_eq!(e.name, "decision.created");
        assert_eq!(e.payload["prev"], serde_json::Value::Null);
        // Supersede folds into one created carrying prev.
        let e = store.set_decision(&id, decision("d2")).unwrap();
        assert_eq!(e.payload["prev"], serde_json::json!("d1"));
        // Stale id never consumes.
        assert!(store.answer_decision(&id, "d1", "once").is_none());
        let e = store.answer_decision(&id, "d2", "once").unwrap();
        assert_eq!(e.name, "decision.answered");
        // Answered twice is a no-op (caller reports answered:false).
        assert!(store.answer_decision(&id, "d2", "once").is_none());
        assert!(store.clear_decision(&id, "test").is_none());
        // Clearing an absent decision on an unknown pane is a no-op.
        assert!(store.clear_decision("pane_nope", "test").is_none());
        store.set_decision(&id, decision("d3")).unwrap();
        let e = store.clear_decision(&id, "attention_cleared").unwrap();
        assert_eq!(e.name, "decision.cleared");
        assert_eq!(e.payload["reason"], serde_json::json!("attention_cleared"));
    }

    #[test]
    fn clear_attention_emits_only_when_set() {
        let mut store = Store::new();
        let p = pane_with(Attention::Unread, 0);
        let id = p.id.clone();
        store.panes.insert(id.clone(), p);
        let e = store.clear_attention(&id, "test").unwrap();
        assert_eq!(e.name, "attention.cleared");
        assert!(store.clear_attention(&id, "test").is_none());
        assert!(store.clear_attention("pane_nope", "test").is_none());
    }
}
