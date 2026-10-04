//! Lifecycle (what the agent is doing) and Attention (whether the
//! human must look) are independent axes. See docs/03.

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// What the agent is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Unknown,
    Working,
    Blocked,
    Done,
    Idle,
    Failed,
    Exited,
}

impl Lifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Lifecycle::Unknown => "unknown",
            Lifecycle::Working => "working",
            Lifecycle::Blocked => "blocked",
            Lifecycle::Done => "done",
            Lifecycle::Idle => "idle",
            Lifecycle::Failed => "failed",
            Lifecycle::Exited => "exited",
        }
    }

    pub fn parse(s: &str) -> Option<Lifecycle> {
        Some(match s {
            "unknown" => Lifecycle::Unknown,
            "working" => Lifecycle::Working,
            "blocked" => Lifecycle::Blocked,
            "done" => Lifecycle::Done,
            "idle" => Lifecycle::Idle,
            "failed" => Lifecycle::Failed,
            "exited" => Lifecycle::Exited,
            _ => return None,
        })
    }

    /// Event name emitted on entering this state.
    pub fn event_name(self) -> &'static str {
        match self {
            Lifecycle::Unknown => "agent.unknown",
            Lifecycle::Working => "agent.working",
            Lifecycle::Blocked => "agent.blocked",
            Lifecycle::Done => "agent.done",
            Lifecycle::Idle => "agent.idle",
            Lifecycle::Failed => "agent.failed",
            Lifecycle::Exited => "agent.exited",
        }
    }

    /// Rank for "worst lifecycle" roll-ups (sidebar, tabs):
    /// blocked > failed > working > done > idle > unknown/exited.
    pub fn urgency(self) -> u8 {
        match self {
            Lifecycle::Blocked => 5,
            Lifecycle::Failed => 4,
            Lifecycle::Working => 3,
            Lifecycle::Done => 2,
            Lifecycle::Idle => 1,
            Lifecycle::Unknown | Lifecycle::Exited => 0,
        }
    }

    /// Rank for sidebar sort order (docs/14 §1): a finished turn
    /// needs review while a working agent needs nothing, so done
    /// outranks working here — the reverse of [`Self::urgency`].
    /// blocked > failed > done > working > idle > unknown/exited.
    pub fn sidebar_rank(self) -> u8 {
        match self {
            Lifecycle::Blocked => 5,
            Lifecycle::Failed => 4,
            Lifecycle::Done => 3,
            Lifecycle::Working => 2,
            Lifecycle::Idle => 1,
            Lifecycle::Unknown | Lifecycle::Exited => 0,
        }
    }
}

/// Why the human must look. Independent of [`Lifecycle`]: an agent
/// can be working with an unread badge, or blocked with no attention
/// once acknowledged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attention {
    None,
    Unread,
    InputRequired,
    PermissionRequired,
    Warning,
    Error,
}

impl Attention {
    pub fn as_str(self) -> &'static str {
        match self {
            Attention::None => "none",
            Attention::Unread => "unread",
            Attention::InputRequired => "input_required",
            Attention::PermissionRequired => "permission_required",
            Attention::Warning => "warning",
            Attention::Error => "error",
        }
    }

    pub fn parse(s: &str) -> Option<Attention> {
        Some(match s {
            "none" => Attention::None,
            "unread" => Attention::Unread,
            "input_required" => Attention::InputRequired,
            "permission_required" => Attention::PermissionRequired,
            "warning" => Attention::Warning,
            "error" => Attention::Error,
            _ => return None,
        })
    }

    pub fn severity(self) -> u8 {
        match self {
            Attention::None => 0,
            Attention::Unread => 1,
            Attention::Warning => 2,
            Attention::InputRequired => 3,
            Attention::PermissionRequired => 4,
            Attention::Error => 5,
        }
    }

    /// A pane shows its highest outstanding item; raising never lowers.
    pub fn raise(self, other: Attention) -> Attention {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }

    pub fn needs_human(self) -> bool {
        self != Attention::None
    }
}

/// A2A-aligned task lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

impl TaskState {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Working => "working",
            TaskState::InputRequired => "input_required",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Canceled => "canceled",
            TaskState::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<TaskState> {
        Some(match s {
            "pending" => TaskState::Pending,
            "working" => TaskState::Working,
            "input_required" => TaskState::InputRequired,
            "completed" => TaskState::Completed,
            "failed" => TaskState::Failed,
            "canceled" => TaskState::Canceled,
            "rejected" => TaskState::Rejected,
            _ => return None,
        })
    }

    /// Terminal states cannot transition to any other state.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Canceled | TaskState::Rejected
        )
    }

    /// Settled means completed, failed, canceled, rejected, or input_required.
    pub fn is_settled(self) -> bool {
        self.is_terminal() || self == TaskState::InputRequired
    }

    /// Validate whether a transition from `self` to `target` is valid under A2A lifecycle rules.
    pub fn can_transition_to(self, target: TaskState) -> bool {
        if self.is_terminal() {
            return false;
        }
        if self == target {
            return true;
        }
        match self {
            TaskState::Pending => matches!(
                target,
                TaskState::Working
                    | TaskState::Failed
                    | TaskState::Canceled
                    | TaskState::Completed
                    | TaskState::Rejected
            ),
            TaskState::Working => matches!(
                target,
                TaskState::InputRequired
                    | TaskState::Completed
                    | TaskState::Failed
                    | TaskState::Canceled
                    | TaskState::Rejected
            ),
            TaskState::InputRequired => matches!(
                target,
                TaskState::Working
                    | TaskState::Completed
                    | TaskState::Failed
                    | TaskState::Canceled
                    | TaskState::Rejected
            ),
            _ => false,
        }
    }

    /// Turn ended without a report: transitions Working -> InputRequired.
    /// If the task is already terminal (e.g. report arrived first), report wins and state is untouched.
    pub fn on_turn_ended_without_report(self) -> Result<TaskState, CoreError> {
        if self.is_terminal() {
            return Ok(self);
        }
        if self == TaskState::Working {
            return Ok(TaskState::InputRequired);
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_rank_follows_directive_order() {
        use Lifecycle::*;
        let ranked = [Blocked, Failed, Done, Working, Idle, Unknown, Exited];
        let ranks: Vec<u8> = ranked.iter().map(|l| l.sidebar_rank()).collect();
        assert_eq!(ranks, vec![5, 4, 3, 2, 1, 0, 0]);
    }

    #[test]
    fn attention_raise_is_max_by_severity() {
        assert_eq!(Attention::None.raise(Attention::Unread), Attention::Unread);
        assert_eq!(Attention::Error.raise(Attention::Unread), Attention::Error);
        assert_eq!(
            Attention::Unread.raise(Attention::PermissionRequired),
            Attention::PermissionRequired
        );
        assert_eq!(
            Attention::InputRequired.raise(Attention::Warning),
            Attention::InputRequired
        );
    }

    #[test]
    fn lifecycle_roundtrip() {
        for s in [
            "unknown", "working", "blocked", "done", "idle", "failed", "exited",
        ] {
            assert_eq!(Lifecycle::parse(s).unwrap().as_str(), s);
        }
        assert_eq!(Lifecycle::parse("nope"), None);
    }

    #[test]
    fn attention_roundtrip() {
        for s in [
            "none",
            "unread",
            "input_required",
            "permission_required",
            "warning",
            "error",
        ] {
            assert_eq!(Attention::parse(s).unwrap().as_str(), s);
        }
    }

    #[test]
    fn task_state_lifecycle_transitions() {
        assert_eq!(TaskState::Pending.as_str(), "pending");
        assert_eq!(
            TaskState::parse("input_required"),
            Some(TaskState::InputRequired)
        );
        assert!(TaskState::Completed.is_terminal());
        assert!(TaskState::Failed.is_terminal());
        assert!(TaskState::Canceled.is_terminal());
        assert!(TaskState::Rejected.is_terminal());
        assert!(!TaskState::Working.is_terminal());
        assert!(!TaskState::InputRequired.is_terminal());

        // Settled
        assert!(TaskState::Completed.is_settled());
        assert!(TaskState::InputRequired.is_settled());
        assert!(!TaskState::Working.is_settled());

        // Valid transitions
        assert!(TaskState::Pending.can_transition_to(TaskState::Working));
        assert!(TaskState::Pending.can_transition_to(TaskState::Completed));
        assert!(TaskState::Working.can_transition_to(TaskState::InputRequired));
        assert!(TaskState::InputRequired.can_transition_to(TaskState::Working));
        assert!(TaskState::Working.can_transition_to(TaskState::Completed));
        assert!(TaskState::Working.can_transition_to(TaskState::Failed));

        // Terminal is immutable
        assert!(!TaskState::Completed.can_transition_to(TaskState::Working));
        assert!(!TaskState::Failed.can_transition_to(TaskState::InputRequired));
        assert!(!TaskState::Canceled.can_transition_to(TaskState::Working));
        assert!(!TaskState::Rejected.can_transition_to(TaskState::Pending));

        // Turn ended without report
        assert_eq!(
            TaskState::Working.on_turn_ended_without_report().unwrap(),
            TaskState::InputRequired
        );
        // Report wins over turn ended
        assert_eq!(
            TaskState::Completed.on_turn_ended_without_report().unwrap(),
            TaskState::Completed
        );
    }
}
