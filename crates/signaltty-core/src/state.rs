//! Lifecycle (what the agent is doing) and Attention (whether the
//! human must look) are independent axes. See docs/03.

use serde::{Deserialize, Serialize};

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

/// Whether the human needs to look. Severity order:
/// error > permission_required > input_required > warning > unread > none.
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
}
