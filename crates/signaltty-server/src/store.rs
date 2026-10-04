//! In-memory authoritative store. Uses std RwLock so both tokio tasks
//! and PTY pump threads can share it. Hold locks briefly, never across await.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use signaltty_core::model::{
    Disposition, DispositionOutcome, Notification, Pane, Tab, Task, TaskResult, Workspace,
};
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_proto::{code, event};

use crate::audit::{AuditLog, EventSequence};

pub const MAX_NOTIFICATIONS: usize = 200;
pub const MAX_EVENTS: usize = 1024;

#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub seq: u64,
    pub name: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitBaseline {
    pub pane_id: String,
    pub process_instance: String,
    pub agent_session_id: Option<String>,
    pub session_generation: u64,
    pub lifecycle_seq: u64,
    pub attention_seq: u64,
}

impl WaitBaseline {
    pub fn threshold(&self, outcome: &str) -> u64 {
        if Attention::parse(outcome).is_some() || matches!(outcome, "seen" | "attention_cleared") {
            self.attention_seq
        } else {
            self.lifecycle_seq
        }
    }
}

struct PaneProgress {
    process_instance: String,
    session_generation: u64,
    lifecycle_seq: u64,
    attention_seq: u64,
    transitions: HashMap<&'static str, u64>,
}

pub struct Store {
    pub workspaces: HashMap<String, Workspace>,
    pub tabs: HashMap<String, Tab>,
    pub panes: HashMap<String, Pane>,
    pub tasks: HashMap<String, Task>,
    pub notifications: VecDeque<Notification>,
    pub events: VecDeque<StoredEvent>,
    pub seq: u64,
    pub started_at: DateTime<Utc>,
    audit: Option<AuditLog>,
    sequence: Option<EventSequence>,
    publisher: Option<tokio::sync::broadcast::Sender<StoredEvent>>,
    ring_retained_after: u64,
    progress: HashMap<String, PaneProgress>,
    pub task_reservations: usize,
}

/// `status_reason.reason` of a task whose prompt was written but never
/// confirmed by agent activity.
pub const SUBMIT_UNCONFIRMED: &str = "submit_unconfirmed";

impl Store {
    pub fn new() -> Store {
        Store {
            workspaces: HashMap::new(),
            tabs: HashMap::new(),
            panes: HashMap::new(),
            tasks: HashMap::new(),
            notifications: VecDeque::new(),
            events: VecDeque::new(),
            seq: 0,
            started_at: Utc::now(),
            audit: None,
            sequence: None,
            publisher: None,
            ring_retained_after: 0,
            progress: HashMap::new(),
            task_reservations: 0,
        }
    }

    pub fn configure_events(
        &mut self,
        log: AuditLog,
        sequence: EventSequence,
        publisher: tokio::sync::broadcast::Sender<StoredEvent>,
    ) {
        self.seq = sequence.head();
        self.ring_retained_after = self.seq;
        self.sequence = Some(sequence);
        self.audit = Some(log);
        self.publisher = Some(publisher);
        for id in self.panes.keys().cloned().collect::<Vec<_>>() {
            self.begin_process(&id);
        }
    }

    fn begin_process(&mut self, pane_id: &str) {
        self.progress.insert(
            pane_id.into(),
            PaneProgress {
                process_instance: signaltty_core::ids::new_pane_id(),
                session_generation: 0,
                lifecycle_seq: self.seq,
                attention_seq: self.seq,
                transitions: HashMap::from([
                    (self.panes[pane_id].lifecycle.as_str(), self.seq),
                    (self.panes[pane_id].attention.as_str(), self.seq),
                ]),
            },
        );
    }

    pub fn publish_pane(&mut self, pane: &Pane, resumed: bool) -> StoredEvent {
        let mut payload = json!({"pane":pane});
        if resumed {
            payload["resumed"] = json!(true);
        }
        let event = self.emit(signaltty_proto::event::PANE_CREATED, payload);
        self.begin_process(&pane.id);
        event
    }

    pub fn remove_pane(&mut self, pane_id: &str) -> Option<Pane> {
        let pane = self.panes.remove(pane_id)?;
        self.progress.remove(pane_id);
        self.emit(
            signaltty_proto::event::PANE_CLOSED,
            json!({"pane_id":pane_id}),
        );
        // Closing the worker pane fails its non-terminal task exactly once:
        // `task_fail` is a no-op on terminal tasks, so whichever of
        // `on_exit`/`remove_pane` runs first wins and the second is silent.
        // (Discard/cancel transition first, so their closes are no-ops here.)
        let task_id = self
            .tasks
            .values()
            .find(|t| t.pane_id.as_deref() == Some(pane_id) && !t.state.is_terminal())
            .map(|t| t.id.clone());
        if let Some(tid) = task_id {
            self.task_fail(&tid, Some(json!({"reason": "pane_closed"})));
        }
        Some(pane)
    }

    pub fn set_exited(&mut self, pane_id: &str, code: Option<i32>) {
        let Some(pane) = self.panes.get_mut(pane_id) else {
            return;
        };
        // Idempotent: the PTY reader's late on_exit after a server-side close
        // must not re-raise attention the closer already cleared.
        if !matches!(pane.live, signaltty_core::model::LiveState::Live) {
            return;
        }
        pane.live = signaltty_core::model::LiveState::Exited { code };
        pane.restore_state = signaltty_core::model::RestoreState::Exited;
        pane.last_activity_at = Utc::now();
        let event = self.emit(
            signaltty_proto::event::PANE_EXITED,
            json!({"pane_id":pane_id,"code":code}),
        );
        if let Some(progress) = self.progress.get_mut(pane_id) {
            progress.lifecycle_seq = event.seq;
            progress.transitions.insert("exited", event.seq);
        }
        self.set_lifecycle(pane_id, Lifecycle::Exited);
        // An exited pane can no longer take input or grant permission.
        if matches!(
            self.panes.get(pane_id).map(|p| p.attention),
            Some(Attention::InputRequired | Attention::PermissionRequired)
        ) {
            self.clear_attention(pane_id, "pane_exited");
        }
        self.raise_attention(pane_id, Attention::Unread);
        self.clear_decision(pane_id, "pane_exited");

        let task_id = self
            .tasks
            .values()
            .find(|t| t.pane_id.as_deref() == Some(pane_id) && !t.state.is_terminal())
            .map(|t| t.id.clone());
        if let Some(tid) = task_id {
            self.task_fail(
                &tid,
                Some(json!({"reason": "pane_exited", "exit_code": code})),
            );
        }
    }

    pub fn matching_transition(&self, pane_id: &str, outcome: &str) -> Option<u64> {
        let key = if matches!(outcome, "seen" | "attention_cleared") {
            "none"
        } else {
            outcome
        };
        self.progress.get(pane_id)?.transitions.get(key).copied()
    }

    pub fn wait_baseline(&self, pane_id: &str) -> Option<WaitBaseline> {
        let pane = self.panes.get(pane_id)?;
        let progress = self.progress.get(pane_id)?;
        Some(WaitBaseline {
            pane_id: pane_id.into(),
            process_instance: progress.process_instance.clone(),
            agent_session_id: pane.agent.agent_session_id.clone(),
            session_generation: progress.session_generation,
            lifecycle_seq: progress.lifecycle_seq,
            attention_seq: progress.attention_seq,
        })
    }

    pub fn set_agent_session(&mut self, pane_id: &str, session_id: String) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        if pane.agent.agent_session_id.as_ref() == Some(&session_id) {
            return None;
        }
        let replacement = pane.agent.agent_session_id.is_some();
        pane.agent.agent_session_id = Some(session_id);
        if replacement {
            if let Some(progress) = self.progress.get_mut(pane_id) {
                progress.session_generation += 1;
            }
        }
        let pane = pane.clone();
        Some(self.emit(signaltty_proto::event::PANE_UPDATED, json!({"pane":pane})))
    }

    /// `task_ids` scopes `task.*` events to those tasks; the filter runs before
    /// the cap and the `returned` count so unrelated traffic cannot truncate.
    pub fn replay(
        &self,
        after: u64,
        patterns: &[String],
        task_ids: Option<&[String]>,
    ) -> (Value, Vec<StoredEvent>) {
        let history = self.audit.as_ref().map(AuditLog::history);
        let retained_after = history
            .as_ref()
            .filter(|h| h.available)
            .map(|h| h.retained_after.min(self.ring_retained_after))
            .unwrap_or(self.ring_retained_after);
        let disk_proves = history
            .as_ref()
            .map(|h| h.available && after >= h.retained_after)
            .unwrap_or(false);
        let ring_proves = after >= self.ring_retained_after;
        let mut status = if after > self.seq {
            "cursor_ahead"
        } else if disk_proves || ring_proves {
            "complete"
        } else if history.as_ref().map(|h| h.available).unwrap_or(false) {
            "history_lost"
        } else {
            "unavailable"
        };
        let mut events = Vec::new();
        if status == "complete" {
            let backfill = if disk_proves {
                history.unwrap().events
            } else {
                Vec::new()
            };
            events = crate::audit::merge_replay(backfill, self.events_since(after))
                .into_iter()
                .filter(|e| {
                    e.seq > after
                        && e.seq <= self.seq
                        && patterns
                            .iter()
                            .any(|g| signaltty_proto::glob_matches(g, &e.name))
                        && task_ids.is_none_or(|tids| {
                            !e.name.starts_with("task.")
                                || e.payload
                                    .get("task_id")
                                    .and_then(|v| v.as_str())
                                    .is_some_and(|tid| tids.iter().any(|t| t == tid))
                        })
                })
                .collect();
            if events.len() > crate::audit::REPLAY_CAP {
                status = "truncated";
                events.clear();
            }
        }
        let mut coverage = json!({"status":status, "requested_after":after, "retained_after":retained_after, "through":self.seq, "returned":events.len()});
        if status != "complete" {
            coverage["recovery"] = json!("snapshot_then_resubscribe");
        }
        (coverage, events)
    }

    /// Record and publish under the assigning Store lock.
    pub fn emit(&mut self, name: &str, payload: Value) -> StoredEvent {
        self.next_seq();
        let ev = StoredEvent {
            seq: self.seq,
            name: name.to_string(),
            payload,
        };
        self.events.push_back(ev.clone());
        while self.events.len() > MAX_EVENTS {
            if let Some(dropped) = self.events.pop_front() {
                self.ring_retained_after = dropped.seq;
            }
        }
        // Sync before publication so replay can prove state-event coverage.
        if let Some(audit) = &self.audit {
            journal_or_exit(audit.append(ev.seq, &ev.name, &ev.payload));
        }
        if let Some(publisher) = &self.publisher {
            let _ = publisher.send(ev.clone());
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
        self.seq = match &mut self.sequence {
            Some(sequence) => journal_or_exit(sequence.allocate()),
            None => self.seq.checked_add(1).expect("event sequence exhausted"),
        };
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

fn journal_or_exit<T>(result: std::io::Result<T>) -> T {
    result.unwrap_or_else(|error| {
        tracing::error!("event journal failed; refusing further event issuance: {error}");
        std::process::exit(1)
    })
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedStore = Arc<RwLock<Store>>;

pub struct TaskReservation {
    store: SharedStore,
    active: bool,
}

impl TaskReservation {
    pub fn commit(mut self, task: Task) -> StoredEvent {
        let mut s = self.store.write().unwrap();
        s.task_reservations = s.task_reservations.saturating_sub(1);
        let ev = s.task_create(task);
        self.active = false;
        ev
    }
}

impl Drop for TaskReservation {
    fn drop(&mut self) {
        if self.active {
            if let Ok(mut s) = self.store.write() {
                s.task_reservations = s.task_reservations.saturating_sub(1);
            }
        }
    }
}

impl Store {
    /// Set lifecycle and publish its paired event only when changed.
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
        if let Some(progress) = self.progress.get_mut(pane_id) {
            progress.lifecycle_seq = ev.seq;
            progress.transitions.insert(next.as_str(), ev.seq);
        }
        // A worker that starts after its submit went unconfirmed (late Enter,
        // slow hooks) took the prompt after all: back to working. A turn that
        // already ended refreshes the evidence to turn_ended_without_report.
        if matches!(next, Lifecycle::Working | Lifecycle::Done) {
            let unconfirmed = self
                .tasks
                .values()
                .find(|t| {
                    t.pane_id.as_deref() == Some(pane_id)
                        && t.state == TaskState::InputRequired
                        && t.status_reason.as_ref().and_then(|r| r.get("reason"))
                            == Some(&json!(SUBMIT_UNCONFIRMED))
                })
                .map(|t| t.id.clone());
            if let Some(tid) = unconfirmed {
                self.task_resume_working(&tid);
            }
        }
        Some(ev)
    }

    /// Raise attention and publish its paired event only when more severe.
    pub fn raise_attention(&mut self, pane_id: &str, next: Attention) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        let prev = pane.attention;
        let raised = prev.raise(next);
        if raised == prev {
            return None;
        }
        let now = Utc::now();
        pane.attention = raised;
        pane.attention_since = Some(now);
        pane.last_activity_at = now;
        let name = if prev == Attention::None {
            signaltty_proto::event::ATTENTION_CREATED
        } else {
            signaltty_proto::event::ATTENTION_UPDATED
        };
        let ev = self.emit(
            name,
            serde_json::json!({"pane_id": pane_id, "attention": raised.as_str(), "prev": prev.as_str()}),
        );
        if let Some(progress) = self.progress.get_mut(pane_id) {
            progress.attention_seq = ev.seq;
            progress.transitions.insert(raised.as_str(), ev.seq);
        }
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

        let task_id = self
            .tasks
            .values()
            .find(|t| t.pane_id.as_deref() == Some(pane_id) && t.state == TaskState::Working)
            .map(|t| t.id.clone());
        if let Some(tid) = task_id {
            self.task_input_required_on_decision(&tid, &decision.id);
        }

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

        let task_id = self
            .tasks
            .values()
            .find(|t| t.pane_id.as_deref() == Some(pane_id) && t.state == TaskState::InputRequired)
            .map(|t| t.id.clone());
        if let Some(tid) = task_id {
            self.task_resume_working(&tid);
        }

        Some(ev)
    }

    /// Drop the pending decision without answering. `None` when absent.
    ///
    /// Never resumes a worker task: only an explicit answer (via
    /// `answer_decision`) or an accepted follow-up submit moves an
    /// `input_required` task back to `working`. Timeout, disconnect
    /// (`native_cancelled`) and turn-end (`moved_on`) drops park the task
    /// in `input_required` so the stall is visible instead of silently
    /// resuming unanswered work.
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

    /// Acknowledge reading without resolving an unanswered decision.
    pub fn mark_seen(&mut self, pane_id: &str, reason: &str) -> Option<StoredEvent> {
        let pane = self.panes.get_mut(pane_id)?;
        pane.last_seen_at = Some(chrono::Utc::now());
        if pane.pending_decision.is_some() {
            return None;
        }
        self.clear_attention(pane_id, reason)
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
        if let Some(progress) = self.progress.get_mut(pane_id) {
            progress.attention_seq = ev.seq;
            progress.transitions.insert("none", ev.seq);
        }
        Some(ev)
    }

    pub fn reserve_task_slot(
        store: &SharedStore,
        max: usize,
    ) -> Result<TaskReservation, (usize, usize)> {
        let mut s = store.write().unwrap();
        let active =
            s.tasks.values().filter(|t| !t.state.is_terminal()).count() + s.task_reservations;
        if active >= max {
            return Err((active, max));
        }
        s.task_reservations += 1;
        Ok(TaskReservation {
            store: Arc::clone(store),
            active: true,
        })
    }

    pub fn task_create(&mut self, task: Task) -> StoredEvent {
        let payload = json!({
            "task_id": task.id,
            "context_id": task.context_id,
            "task": task,
        });
        self.tasks.insert(task.id.clone(), task);
        self.emit(event::TASK_CREATED, payload)
    }

    fn emit_task_updated(&mut self, task: &Task, prev_state: Option<TaskState>) -> StoredEvent {
        let mut payload = json!({
            "task_id": task.id,
            "context_id": task.context_id,
            "task": task,
        });
        if let Some(prev) = prev_state {
            payload["prev_state"] = json!(prev.as_str());
        }
        self.emit(event::TASK_UPDATED, payload)
    }

    pub fn task_background_ready(&mut self, task_id: &str) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            if task.state != TaskState::Pending {
                return None;
            }
            let prev_state = task.state;
            let now = Utc::now();
            task.transition_to(TaskState::Working, now).ok()?;
            // A Stop that landed before the submit committed leaves the pane
            // lifecycle done with no report: park in input_required here, in
            // the same write, instead of a working state nobody will end.
            // Idle is not turn end.
            if let Some(evidence) = Self::turn_end_evidence(&self.panes, task) {
                task.state = TaskState::InputRequired;
                task.status_reason = Some(evidence);
                task.updated_at = now;
            }
            (task.clone(), prev_state)
        };
        let ev = self.emit_task_updated(&task_clone, Some(prev_state));
        Some(ev)
    }

    /// Evidence for a turn that ended with no report: the worker pane
    /// lifecycle is already `done` while the task holds no result.
    /// `None` when the pane is still live-turning, idle, or reported.
    fn turn_end_evidence(panes: &HashMap<String, Pane>, task: &Task) -> Option<Value> {
        if task.result.is_some() {
            return None;
        }
        let pane = task.pane_id.as_deref().and_then(|pid| panes.get(pid))?;
        if pane.lifecycle != Lifecycle::Done {
            return None;
        }
        Some(json!({
            "reason": "turn_ended_without_report",
            "last_message": pane.last_message,
        }))
    }

    pub fn task_report(
        &mut self,
        task_id: &str,
        result: TaskResult,
    ) -> Result<(StoredEvent, StoredEvent), (String, String)> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id).ok_or_else(|| {
                (
                    code::NO_SUCH_TASK.to_string(),
                    format!("no such task '{task_id}'"),
                )
            })?;
            let prev_state = task.state;
            let now = Utc::now();
            task.apply_report(result.clone(), now).map_err(|e| {
                (
                    code::BAD_PARAMS.to_string(),
                    format!("failed to apply task report: {e}"),
                )
            })?;
            (task.clone(), prev_state)
        };
        let result_ev = self.emit(
            event::TASK_RESULT,
            json!({
                "task_id": task_clone.id,
                "context_id": task_clone.context_id,
                "result": result,
            }),
        );
        let update_ev = self.emit_task_updated(&task_clone, Some(prev_state));
        Ok((result_ev, update_ev))
    }

    pub fn task_cancel(&mut self, task_id: &str) -> Result<StoredEvent, (String, String)> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id).ok_or_else(|| {
                (
                    code::NO_SUCH_TASK.to_string(),
                    format!("no such task '{task_id}'"),
                )
            })?;
            if task.state.is_terminal() {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    format!("task is already terminal ({})", task.state.as_str()),
                ));
            }
            let prev_state = task.state;
            let now = Utc::now();
            task.transition_to(TaskState::Canceled, now).map_err(|e| {
                (
                    code::BAD_PARAMS.to_string(),
                    format!("cannot cancel task: {e}"),
                )
            })?;
            (task.clone(), prev_state)
        };
        let ev = self.emit_task_updated(&task_clone, Some(prev_state));
        Ok(ev)
    }

    /// The background submit wrote the prompt but saw no activity: park in
    /// `input_required` instead of `failed`, so a nudge (`pane.submit`) or
    /// the worker starting late resumes it and its work stays finishable.
    pub fn task_submit_unconfirmed(&mut self, task_id: &str, error: &str) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            if task.state != TaskState::Pending {
                return None;
            }
            let prev_state = task.state;
            let now = Utc::now();
            task.transition_to(TaskState::Working, now).ok()?;
            // Activity that landed right after the gate's deadline confirms it.
            let started = task
                .pane_id
                .as_deref()
                .and_then(|pid| self.panes.get(pid))
                .is_some_and(|p| matches!(p.lifecycle, Lifecycle::Working | Lifecycle::Blocked));
            if started {
                (task.clone(), prev_state)
            } else {
                task.transition_to(TaskState::InputRequired, now).ok()?;
                task.status_reason = Some(json!({
                    "reason": SUBMIT_UNCONFIRMED,
                    "stage": "activity_gate",
                    "error": error,
                }));
                (task.clone(), prev_state)
            }
        };
        Some(self.emit_task_updated(&task_clone, Some(prev_state)))
    }

    pub fn task_fail(&mut self, task_id: &str, evidence: Option<Value>) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            if task.state.is_terminal() {
                return None;
            }
            let prev_state = task.state;
            let now = Utc::now();
            task.transition_to(TaskState::Failed, now).ok()?;
            if evidence.is_some() {
                task.status_reason = evidence;
            }
            (task.clone(), prev_state)
        };
        let ev = self.emit_task_updated(&task_clone, Some(prev_state));
        Some(ev)
    }

    pub fn task_finish_record(
        &mut self,
        task_id: &str,
        disposition: Disposition,
        finish_error: Option<Value>,
    ) -> Result<StoredEvent, (String, String)> {
        let task_clone = {
            let task = self.tasks.get_mut(task_id).ok_or_else(|| {
                (
                    code::NO_SUCH_TASK.to_string(),
                    format!("no such task '{task_id}'"),
                )
            })?;
            if task.disposition.outcome != DispositionOutcome::None {
                return Err((
                    code::BAD_PARAMS.to_string(),
                    "task disposition has already been recorded".to_string(),
                ));
            }
            task.disposition = disposition;
            task.finish_error = finish_error;
            task.updated_at = Utc::now();
            task.clone()
        };
        let ev = self.emit_task_updated(&task_clone, None);
        Ok(ev)
    }

    /// Replace finish evidence and emit `task.updated` with `task_id`.
    /// `None` clears a previous cleanup or conflict note. Does not change
    /// disposition, so a recorded finish can retry cleanup.
    pub fn task_set_finish_error(
        &mut self,
        task_id: &str,
        finish_error: Option<Value>,
    ) -> Result<StoredEvent, (String, String)> {
        let task_clone = {
            let task = self.tasks.get_mut(task_id).ok_or_else(|| {
                (
                    code::NO_SUCH_TASK.to_string(),
                    format!("no such task '{task_id}'"),
                )
            })?;
            task.finish_error = finish_error;
            task.updated_at = Utc::now();
            task.clone()
        };
        Ok(self.emit_task_updated(&task_clone, None))
    }

    pub fn task_input_required_on_turn_end(
        &mut self,
        task_id: &str,
        evidence: Option<Value>,
    ) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            let prev_state = task.state;
            let now = Utc::now();
            if !task.apply_turn_ended_without_report(evidence, now) {
                return None;
            }
            (task.clone(), prev_state)
        };
        let ev = self.emit_task_updated(&task_clone, Some(prev_state));
        Some(ev)
    }

    pub fn task_for_pane(&self, pane_id: &str) -> Option<&Task> {
        self.tasks
            .values()
            .find(|t| t.pane_id.as_deref() == Some(pane_id))
    }

    pub fn task_for_pane_mut(&mut self, pane_id: &str) -> Option<&mut Task> {
        self.tasks
            .values_mut()
            .find(|t| t.pane_id.as_deref() == Some(pane_id))
    }

    pub fn task_resume_working(&mut self, task_id: &str) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            if task.state != TaskState::InputRequired {
                return None;
            }
            let now = Utc::now();
            // A follow-up whose Stop arrives before the resume commits must
            // land: refresh the turn-end evidence instead of going working.
            if let Some(evidence) = Self::turn_end_evidence(&self.panes, task) {
                task.status_reason = Some(evidence);
                task.updated_at = now;
                (task.clone(), None)
            } else {
                let prev_state = task.state;
                task.transition_to(TaskState::Working, now).ok()?;
                // The interrupt is resolved; its evidence is stale on a working task.
                task.status_reason = None;
                task.updated_at = now;
                (task.clone(), Some(prev_state))
            }
        };
        Some(self.emit_task_updated(&task_clone, prev_state))
    }

    /// A pending decision blocks the worker: `working` → `input_required`
    /// with the decision as evidence. Only fires from `working` (a `pending`
    /// task's first submit is still in flight; terminal tasks never move).
    pub fn task_input_required_on_decision(
        &mut self,
        task_id: &str,
        decision_id: &str,
    ) -> Option<StoredEvent> {
        let (task_clone, prev_state) = {
            let task = self.tasks.get_mut(task_id)?;
            if task.state != TaskState::Working {
                return None;
            }
            let prev_state = task.state;
            let now = Utc::now();
            task.transition_to(TaskState::InputRequired, now).ok()?;
            task.status_reason = Some(json!({
                "reason": "decision_required",
                "decision_id": decision_id,
            }));
            task.updated_at = now;
            (task.clone(), prev_state)
        };
        Some(self.emit_task_updated(&task_clone, Some(prev_state)))
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
    fn replay_filters_before_cap_and_reports_incomplete_history() {
        let d = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
        let log = AuditLog::open(&d).unwrap();
        let sequence = EventSequence::open(&d, 0).unwrap();
        let (publisher, _) = tokio::sync::broadcast::channel(8);
        let mut store = Store::new();
        store.configure_events(log, sequence, publisher);
        for _ in 0..=crate::audit::REPLAY_CAP {
            store.emit("excluded", Value::Null);
        }
        store.next_seq(); // PTY numbers are legitimate holes.
        store.emit("included", Value::Null);
        let (coverage, events) = store.replay(0, &["included".into()], None);
        assert_eq!(coverage["status"], "complete");
        assert_eq!(events.len(), 1);
        let (coverage, events) = store.replay(0, &["*".into()], None);
        assert_eq!(coverage["status"], "truncated");
        assert!(events.is_empty());
        assert_eq!(
            store.replay(store.seq + 1, &["*".into()], None).0["status"],
            "cursor_ahead"
        );
        store.audit = None;
        assert_eq!(
            store.replay(0, &["*".into()], None).0["status"],
            "unavailable"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn replay_applies_task_filter_before_cap_and_count() {
        let d = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
        let log = AuditLog::open(&d).unwrap();
        let sequence = EventSequence::open(&d, 0).unwrap();
        let (publisher, _) = tokio::sync::broadcast::channel(8);
        let mut store = Store::new();
        store.configure_events(log, sequence, publisher);
        for _ in 0..=crate::audit::REPLAY_CAP {
            store.emit("task.updated", json!({"task_id": "other"}));
        }
        store.emit("task.updated", json!({"task_id": "mine"}));
        store.emit("pane.updated", Value::Null);
        let mine = vec!["mine".to_string()];
        let (coverage, events) = store.replay(0, &["*".into()], Some(&mine));
        assert_eq!(coverage["status"], "complete");
        assert_eq!(coverage["returned"], 2);
        assert_eq!(events.len(), 2);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn lifecycle_baseline_is_independent_of_attention_and_same_state_hooks() {
        let mut store = Store::new();
        let pane = pane_with(Attention::None, 0);
        let id = pane.id.clone();
        store.panes.insert(id.clone(), pane.clone());
        store.publish_pane(&pane, false);
        store.set_lifecycle(&id, Lifecycle::Done);
        let old = store.wait_baseline(&id).unwrap();
        store.raise_attention(&id, Attention::Unread);
        store.set_lifecycle(&id, Lifecycle::Done);
        let current = store.wait_baseline(&id).unwrap();
        assert_eq!(current.lifecycle_seq, old.lifecycle_seq);
        assert!(current.attention_seq > old.attention_seq);
        store.set_agent_session(&id, "one".into());
        assert_eq!(store.wait_baseline(&id).unwrap().session_generation, 0);
        store.set_agent_session(&id, "two".into());
        store.set_agent_session(&id, "one".into());
        assert_eq!(store.wait_baseline(&id).unwrap().session_generation, 2);
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

    #[test]
    fn exit_demotes_input_attention_and_is_idempotent() {
        let mut store = Store::new();
        let p = pane_with(Attention::InputRequired, 0);
        let id = p.id.clone();
        store.panes.insert(id.clone(), p);
        store.set_exited(&id, Some(0));
        assert_eq!(store.panes[&id].attention, Attention::Unread);
        // A late PTY on_exit after a server-side close re-raises nothing.
        store.clear_attention(&id, "task_closed");
        store.set_exited(&id, Some(0));
        assert_eq!(store.panes[&id].attention, Attention::None);
    }

    #[test]
    fn task_store_transition_methods_pair_mutate_and_emit() {
        use signaltty_core::model::{Contract, Relationship, TaskResultStatus};
        use std::path::PathBuf;

        let mut store = Store::new();
        let now = Utc::now();
        let contract = Contract::new("Do work").unwrap();
        let task_id = signaltty_core::ids::new_task_id();
        let task = Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some("p1".to_string()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "store-test".to_string(),
            contract,
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: Some("main".to_string()),
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-1".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "1234abcd".to_string(),
            state: TaskState::Pending,
            result: None,
            disposition: Disposition::default(),
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: now,
            updated_at: now,
        };

        // 1. Create
        let ev = store.task_create(task);
        assert_eq!(ev.name, "task.created");
        assert_eq!(ev.payload["task_id"], task_id.as_str());
        assert_eq!(
            ev.payload["context_id"],
            store.tasks[&task_id].context_id.as_str()
        );
        assert!(store.tasks.contains_key(&task_id));

        // 2. Background ready
        let ev = store.task_background_ready(&task_id).unwrap();
        assert_eq!(ev.name, "task.updated");
        assert_eq!(store.tasks[&task_id].state, TaskState::Working);

        // 3. Turn end without report
        let ev = store
            .task_input_required_on_turn_end(
                &task_id,
                Some(serde_json::json!({"reason": "turn_ended_without_report"})),
            )
            .unwrap();
        assert_eq!(ev.name, "task.updated");
        assert_eq!(store.tasks[&task_id].state, TaskState::InputRequired);

        // 4. Report
        let result = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "Finished".to_string(),
            artifacts: vec![],
            evidence: None,
            reported_at: Utc::now(),
        };
        let (res_ev, upd_ev) = store.task_report(&task_id, result).unwrap();
        assert_eq!(res_ev.name, "task.result");
        assert_eq!(upd_ev.name, "task.updated");
        assert_eq!(store.tasks[&task_id].state, TaskState::Completed);

        // Report on terminal task is refused
        assert!(store
            .task_report(
                &task_id,
                TaskResult {
                    status: TaskResultStatus::Completed,
                    summary: "Again".to_string(),
                    artifacts: vec![],
                    evidence: None,
                    reported_at: Utc::now(),
                }
            )
            .is_err());

        // Cancel on terminal task is refused
        assert!(store.task_cancel(&task_id).is_err());

        // 5. Finish record
        let disp = Disposition {
            outcome: DispositionOutcome::Merged,
            target_ref: Some("main".to_string()),
            merged_sha: Some("sha".to_string()),
            branch_deleted: Some(true),
            at: Some(Utc::now()),
        };
        let ev = store
            .task_finish_record(
                &task_id,
                disp.clone(),
                Some(json!({ "cleanup_error": "left in place" })),
            )
            .unwrap();
        assert_eq!(ev.name, "task.updated");
        assert_eq!(ev.payload["task_id"], task_id);
        assert_eq!(
            store.tasks[&task_id].disposition.outcome,
            DispositionOutcome::Merged
        );
        assert_eq!(
            store.tasks[&task_id].finish_error.as_ref().unwrap()["cleanup_error"],
            "left in place"
        );

        // Second finish record refused
        assert!(store.task_finish_record(&task_id, disp, None).is_err());
        // Cleanup evidence can still be replaced so a later finish can retry.
        store.task_set_finish_error(&task_id, None).unwrap();
        assert!(store.tasks[&task_id].finish_error.is_none());
    }

    #[test]
    fn clear_decision_never_resumes_task_for_non_answer_reasons() {
        use signaltty_core::model::{Contract, Decision, DecisionOption, Relationship};
        use std::path::PathBuf;

        let mut store = Store::new();
        let pane = pane_with(Attention::None, 0);
        let pane_id = pane.id.clone();
        store.panes.insert(pane_id.clone(), pane);
        let decision = || Decision {
            id: "d1".to_string(),
            prompt: "Allow?".to_string(),
            options: vec![DecisionOption {
                id: "once".into(),
                label: "Once".into(),
            }],
            answerable: true,
            received_at: Utc::now(),
        };
        let task_id = signaltty_core::ids::new_task_id();
        let task = Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some(pane_id.clone()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "decision-test".to_string(),
            contract: Contract::new("Objective").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-d".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "sha".to_string(),
            state: TaskState::InputRequired,
            result: None,
            disposition: Disposition::default(),
            status_reason: Some(serde_json::json!({"reason": "decision_required"})),
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.task_create(task);

        // Non-answer drops (timeout, disconnect, turn end) must park the
        // task in input_required, never resume it to working unanswered.
        for reason in [
            "native_cancelled",
            "timeout",
            "moved_on",
            "attention_cleared",
        ] {
            store.set_decision(&pane_id, decision());
            // set_decision only moves working -> input_required; force it here.
            store.tasks.get_mut(&task_id).unwrap().state = TaskState::InputRequired;
            store.clear_decision(&pane_id, reason);
            assert_eq!(
                store.tasks[&task_id].state,
                TaskState::InputRequired,
                "clear_decision({reason}) must not resume the task"
            );
        }
        // The answer path still resumes.
        store.set_decision(&pane_id, decision());
        store.tasks.get_mut(&task_id).unwrap().state = TaskState::InputRequired;
        store.answer_decision(&pane_id, "d1", "once");
        assert_eq!(store.tasks[&task_id].state, TaskState::Working);
    }

    #[test]
    fn background_ready_applies_turn_end_when_pane_already_done() {
        use signaltty_core::model::{Contract, Relationship};
        use std::path::PathBuf;

        let mut store = Store::new();
        let mut pane = pane_with(Attention::None, 0);
        pane.lifecycle = Lifecycle::Done;
        pane.last_message = Some("half an answer".to_string());
        let pane_id = pane.id.clone();
        store.panes.insert(pane_id.clone(), pane);
        let task_id = signaltty_core::ids::new_task_id();
        let mk = |state: TaskState| Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some(pane_id.clone()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "turn-end".to_string(),
            contract: Contract::new("Objective").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-t".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "sha".to_string(),
            state,
            result: None,
            disposition: Disposition::default(),
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        // Stop landed while the task was still pending: entering working
        // must apply turn_ended_without_report in the same write.
        store.task_create(mk(TaskState::Pending));
        store.task_background_ready(&task_id).unwrap();
        assert_eq!(store.tasks[&task_id].state, TaskState::InputRequired);
        assert_eq!(
            store.tasks[&task_id].status_reason,
            Some(serde_json::json!({
                "reason": "turn_ended_without_report",
                "last_message": "half an answer",
            }))
        );
        // A follow-up whose Stop arrives before the resume lands back in
        // input_required with fresh turn-end evidence, not working.
        store.tasks.get_mut(&task_id).unwrap().state = TaskState::InputRequired;
        store.task_resume_working(&task_id).unwrap();
        assert_eq!(store.tasks[&task_id].state, TaskState::InputRequired);
        assert_eq!(
            store.tasks[&task_id]
                .status_reason
                .as_ref()
                .and_then(|v| v.get("reason")),
            Some(&serde_json::json!("turn_ended_without_report"))
        );
    }

    #[test]
    fn background_ready_stays_working_when_pane_idle_or_live() {
        use signaltty_core::model::{Contract, Relationship};
        use std::path::PathBuf;

        // Idle is not turn end: a task entering working with an idle pane
        // stays working.
        let mut store = Store::new();
        let mut pane = pane_with(Attention::None, 0);
        pane.lifecycle = Lifecycle::Idle;
        let pane_id = pane.id.clone();
        store.panes.insert(pane_id.clone(), pane);
        let task_id = signaltty_core::ids::new_task_id();
        store.task_create(Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some(pane_id),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "idle".to_string(),
            contract: Contract::new("Objective").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-i".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "sha".to_string(),
            state: TaskState::Pending,
            result: None,
            disposition: Disposition::default(),
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        });
        store.task_background_ready(&task_id).unwrap();
        assert_eq!(store.tasks[&task_id].state, TaskState::Working);
    }

    #[test]
    fn pane_exit_fails_non_terminal_task() {
        use signaltty_core::model::{Contract, Relationship};
        use std::path::PathBuf;

        let mut store = Store::new();
        let pane = pane_with(Attention::None, 0);
        let pane_id = pane.id.clone();
        store.panes.insert(pane_id.clone(), pane);

        let task_id = signaltty_core::ids::new_task_id();
        let task = Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some(pane_id.clone()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "exit-test".to_string(),
            contract: Contract::new("Objective").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-b".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "sha".to_string(),
            state: TaskState::Working,
            result: None,
            disposition: Disposition::default(),
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.task_create(task);

        // Pane exits with code 1
        store.set_exited(&pane_id, Some(1));
        assert_eq!(store.tasks[&task_id].state, TaskState::Failed);
        assert_eq!(
            store.tasks[&task_id].status_reason,
            Some(serde_json::json!({"reason": "pane_exited", "exit_code": 1}))
        );
        // Exit repeated (close-after-exit): no second transition, reason stable.
        store.set_exited(&pane_id, Some(1));
        assert_eq!(store.tasks[&task_id].state, TaskState::Failed);
        assert_eq!(
            store.tasks[&task_id].status_reason,
            Some(serde_json::json!({"reason": "pane_exited", "exit_code": 1}))
        );
    }

    #[test]
    fn pane_close_fails_non_terminal_task_exactly_once() {
        use signaltty_core::model::{Contract, Relationship};
        use std::path::PathBuf;

        let mut store = Store::new();
        let pane = pane_with(Attention::None, 0);
        let pane_id = pane.id.clone();
        store.panes.insert(pane_id.clone(), pane);

        let task_id = signaltty_core::ids::new_task_id();
        let task = Task {
            id: task_id.clone(),
            context_id: signaltty_core::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some(pane_id.clone()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "close-test".to_string(),
            contract: Contract::new("Objective").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "task-b".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "sha".to_string(),
            state: TaskState::Working,
            result: None,
            disposition: Disposition::default(),
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.task_create(task);

        // Operator close fails the task once; exit racing after close is silent.
        store.remove_pane(&pane_id);
        assert_eq!(store.tasks[&task_id].state, TaskState::Failed);
        assert_eq!(
            store.tasks[&task_id].status_reason,
            Some(serde_json::json!({"reason": "pane_closed"}))
        );
        store.set_exited(&pane_id, Some(1));
        assert_eq!(
            store.tasks[&task_id].status_reason,
            Some(serde_json::json!({"reason": "pane_closed"}))
        );
    }
}
