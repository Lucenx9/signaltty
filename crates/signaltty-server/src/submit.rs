//! Gated prompt delivery: readiness check, bracketed-paste wrap, delayed Enter,
//! and activity-gate wait baseline.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use signaltty_core::model::LiveState;
use signaltty_core::state::{Attention, Lifecycle, TaskState};
use signaltty_proto::code;
use tokio::sync::broadcast;

use crate::pty::PtyManager;
use crate::router::Ctx;
use crate::store::{SharedStore, StoredEvent};

/// `pane.submit` body cap. The background worker prompt uses a higher cap
/// ([`worker_submit_max_bytes`]) so a maximum-sized objective still fits
/// after the fixed preamble.
pub const PANE_SUBMIT_MAX_BYTES: usize = 32 * 1024;

/// Bytes the background first submit may write: the pane cap plus the fixed
/// preamble for this task (id, branch, base). A max objective with no extra
/// sections fits; constraints or criteria beyond that do not.
pub fn worker_submit_max_bytes(task_id: &str, branch: &str, base_sha: &str) -> usize {
    let bare = signaltty_core::model::Contract {
        objective: String::new(),
        constraints: None,
        acceptance_criteria: None,
        output_format: None,
    };
    let wrapper = signaltty_core::model::compose_worker_prompt(task_id, branch, base_sha, &bare);
    PANE_SUBMIT_MAX_BYTES + wrapper.len()
}

#[derive(Clone)]
pub struct SubmitCtx {
    pub store: SharedStore,
    pub ptys: PtyManager,
    pub bcast: broadcast::Sender<StoredEvent>,
}

impl From<&Ctx> for SubmitCtx {
    fn from(ctx: &Ctx) -> Self {
        Self {
            store: ctx.store.clone(),
            ptys: ctx.ptys.clone(),
            bcast: ctx.bcast.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitOutcome {
    pub submitted: bool,
    pub outcome: String,
    pub transition_seq: u64,
    pub lifecycle: String,
    pub attention: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl SubmitError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }
}

impl std::fmt::Display for SubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SubmitError {}

impl From<SubmitError> for (String, String) {
    fn from(e: SubmitError) -> Self {
        (e.code, e.message)
    }
}

/// Gated prompt submission:
/// 1. Validate text length (`1..=max_bytes`). `pane.submit` passes 32 KiB.
/// 2. Reject text that already contains `ESC[200~` or `ESC[201~` (`BAD_PARAMS`).
///    Stripping would change the prompt the worker sees, so the text is refused
///    whole, before the readiness gate and before any PTY write.
/// 3. Gate on pane lifecycle (`idle` or `done`) and live state.
///    - If `!allow_pending_task` and worker pane is still `pending`, refuse `AGENT_NOT_READY`.
///    - If working/blocked, refuse `AGENT_BUSY`.
///    - If unknown/failed, refuse `AGENT_NOT_READY`.
///    - If exited, refuse `PANE_EXITED`.
/// 4. Record pre-submit `WaitBaseline`.
/// 5. Write bracketed paste (`\x1b[200~text\x1b[201~`).
/// 6. Sleep `submit_delay`.
/// 7. Recheck live/decision state and write `\r`; a new decision returns
///    `AGENT_BUSY` with `paste_delivered: true`, leaving it unanswered.
/// 8. Activity gate (if `check_activity`): wait up to `stall_timeout` for a newer `working` or `blocked` transition.
/// 9. If worker pane belongs to an `input_required` task, resume it to `working`.
#[allow(clippy::too_many_arguments)]
pub async fn submit_prompt(
    ctx: &SubmitCtx,
    pane_id: &str,
    text: &str,
    submit_delay: Duration,
    stall_timeout: Duration,
    allow_pending_task: bool,
    check_activity: bool,
    max_bytes: usize,
) -> Result<SubmitOutcome, SubmitError> {
    if text.is_empty() || text.len() > max_bytes {
        return Err(SubmitError::new(
            code::BAD_PARAMS,
            format!("text must be 1 .. {max_bytes} bytes"),
        ));
    }
    // Checked before any PTY write so an overflow cannot panic after the
    // prompt was already delivered.
    if check_activity
        && tokio::time::Instant::now()
            .checked_add(stall_timeout)
            .is_none()
    {
        return Err(SubmitError::new(
            code::BAD_PARAMS,
            "stall timeout is too large",
        ));
    }
    // Reject, do not strip. A marker inside the text ends or opens paste mode
    // early, and the tail is delivered as immediate keystrokes. Removing the
    // bytes would silently change the prompt, so the whole text is refused
    // before the readiness gate and before any `ptys.input`.
    if text.contains("\u{1b}[200~") || text.contains("\u{1b}[201~") {
        return Err(SubmitError::new(
            code::BAD_PARAMS,
            "text must not contain bracketed-paste markers ESC[200~ or ESC[201~",
        ));
    }

    let baseline = {
        let s = ctx.store.read().unwrap();
        let pane = s
            .panes
            .get(pane_id)
            .ok_or_else(|| SubmitError::new(code::NO_SUCH_PANE, pane_id.to_string()))?;

        if !matches!(pane.live, LiveState::Live) {
            return Err(SubmitError::new(
                code::PANE_EXITED,
                format!("pane '{pane_id}' has exited"),
            ));
        }

        if !allow_pending_task
            && s.tasks
                .values()
                .any(|t| t.pane_id.as_deref() == Some(pane_id) && t.state == TaskState::Pending)
        {
            return Err(SubmitError::new(
                code::AGENT_NOT_READY,
                "worker pane task is still pending background submit",
            ));
        }

        match pane.lifecycle {
            Lifecycle::Working => {
                return Err(SubmitError::new(
                    code::AGENT_BUSY,
                    format!("agent is {}", pane.lifecycle.as_str()),
                ));
            }
            Lifecycle::Blocked => {
                let can_submit =
                    pane.attention == Attention::InputRequired && pane.pending_decision.is_none();
                if !can_submit {
                    return Err(SubmitError::new(
                        code::AGENT_BUSY,
                        format!("agent is {}", pane.lifecycle.as_str()),
                    ));
                }
            }
            Lifecycle::Unknown | Lifecycle::Failed => {
                return Err(SubmitError::new(
                    code::AGENT_NOT_READY,
                    format!("agent is {}", pane.lifecycle.as_str()),
                ));
            }
            Lifecycle::Exited => {
                return Err(SubmitError::new(
                    code::PANE_EXITED,
                    format!("pane '{pane_id}' has exited"),
                ));
            }
            Lifecycle::Idle | Lifecycle::Done => {
                if pane.pending_decision.is_some()
                    || pane.attention == Attention::PermissionRequired
                {
                    return Err(SubmitError::new(
                        code::AGENT_BUSY,
                        "agent has pending decision or permission required",
                    ));
                }
            }
        }

        s.wait_baseline(pane_id)
    };

    // Bracketed paste wrap: ESC[200~ text ESC[201~
    let paste = format!("\x1b[200~{text}\x1b[201~");
    ctx.ptys
        .input_async(pane_id, paste.into_bytes())
        .await
        .map_err(|e| SubmitError::new(code::IO_ERROR, format!("PTY input error: {e}")))?;

    tokio::time::sleep(submit_delay).await;

    {
        let s = ctx.store.read().unwrap();
        let pane = s
            .panes
            .get(pane_id)
            .ok_or_else(|| SubmitError::new(code::NO_SUCH_PANE, pane_id.to_string()))?;
        if !matches!(pane.live, LiveState::Live) {
            return Err(SubmitError::new(
                code::PANE_EXITED,
                format!("pane '{pane_id}' has exited"),
            ));
        }
        // A permission may arrive while the TUI is settling the paste. Enter
        // now belongs to that decision, not to the prompt we already wrote.
        if pane.pending_decision.is_some()
            || pane.attention == Attention::PermissionRequired
            || (pane.lifecycle == Lifecycle::Blocked && pane.attention != Attention::InputRequired)
        {
            return Err(SubmitError::with_details(
                code::AGENT_BUSY,
                "agent requires a decision after paste; Enter was not sent",
                serde_json::json!({"stage": "delayed_enter", "paste_delivered": true}),
            ));
        }
        // Keep this single-byte write under the decision check's read guard.
        ctx.ptys
            .input(pane_id, b"\r")
            .map_err(|e| SubmitError::new(code::IO_ERROR, format!("PTY input error: {e}")))?;
    }

    if !check_activity {
        let s = ctx.store.read().unwrap();
        let pane = s
            .panes
            .get(pane_id)
            .ok_or_else(|| SubmitError::new(code::NO_SUCH_PANE, pane_id.to_string()))?;
        return Ok(SubmitOutcome {
            submitted: true,
            outcome: "submitted".to_string(),
            transition_seq: baseline.as_ref().map(|b| b.lifecycle_seq).unwrap_or(0),
            lifecycle: pane.lifecycle.as_str().to_string(),
            attention: pane.attention.as_str().to_string(),
        });
    }

    // Activity gate
    let outcomes = ["working", "blocked"];
    let deadline = tokio::time::Instant::now() + stall_timeout; // checked above
    let mut rx = ctx.bcast.subscribe();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    // One Enter retry halfway through: a TUI still settling its input box
    // can drop the first `\r` and leave the prompt typed but unsent.
    let enter_retry = tokio::time::Instant::now() + stall_timeout / 2;
    let mut enter_retried = false;

    loop {
        {
            let s = ctx.store.read().unwrap();
            let Some(pane) = s.panes.get(pane_id) else {
                return Err(SubmitError::new(code::NO_SUCH_PANE, pane_id.to_string()));
            };
            if !matches!(pane.live, LiveState::Live) {
                return Err(SubmitError::new(
                    code::PANE_EXITED,
                    format!("pane '{pane_id}' exited while waiting for activity"),
                ));
            }
            for outcome in &outcomes {
                let transition = s.matching_transition(pane_id, outcome).unwrap_or(0);
                let satisfied = match &baseline {
                    Some(after) => transition > after.threshold(outcome),
                    None => pane.lifecycle.as_str() == *outcome,
                };
                if satisfied {
                    drop(s);
                    // Accepted submit on an input_required worker pane moves task back to working
                    {
                        let mut s = ctx.store.write().unwrap();
                        let target_task_id = s
                            .tasks
                            .values()
                            .find(|t| {
                                t.pane_id.as_deref() == Some(pane_id)
                                    && t.state == TaskState::InputRequired
                            })
                            .map(|t| t.id.clone());
                        if let Some(tid) = target_task_id {
                            s.task_resume_working(&tid);
                        }
                    }
                    let s = ctx.store.read().unwrap();
                    let pane = s
                        .panes
                        .get(pane_id)
                        .ok_or_else(|| SubmitError::new(code::NO_SUCH_PANE, pane_id.to_string()))?;
                    return Ok(SubmitOutcome {
                        submitted: true,
                        outcome: (*outcome).to_string(),
                        transition_seq: transition,
                        lifecycle: pane.lifecycle.as_str().to_string(),
                        attention: pane.attention.as_str().to_string(),
                    });
                }
            }
        }

        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => {
                return Err(SubmitError::with_details(
                    code::TIMEOUT,
                    "activity gate timed out waiting for working or blocked after submit",
                    serde_json::json!({"stage": "activity_gate"}),
                ));
            }
            _ = tokio::time::sleep_until(enter_retry), if !enter_retried => {
                enter_retried = true;
                // Only into an untouched input box: never while a decision
                // or permission prompt could take the Enter as a yes.
                let s = ctx.store.read().unwrap();
                let untouched = s.panes.get(pane_id).is_some_and(|p| {
                    matches!(p.live, LiveState::Live)
                        && matches!(p.lifecycle, Lifecycle::Idle | Lifecycle::Done)
                        && p.pending_decision.is_none()
                        && p.attention != Attention::PermissionRequired
                });
                if untouched {
                    let _ = ctx.ptys.input(pane_id, b"\r");
                }
            }
            _ = tick.tick() => {}
            event = rx.recv() => {
                if matches!(event, Err(broadcast::error::RecvError::Closed)) {
                    return Err(SubmitError::new(code::INTERNAL, "event bus closed"));
                }
            }
        }
    }
}
