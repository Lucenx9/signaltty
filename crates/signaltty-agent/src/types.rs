//! Adapter interface types. Method names follow docs/05 spec:
//! identify / lifecycle_state / session_identity / notification_event /
//! resume_capability / metadata.

use serde_json::Value;
use signaltty_core::model::{AgentKind, NotificationSeverity};
use signaltty_core::state::{Attention, Lifecycle};

/// What the adapter knows about the pane's foreground process.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    pub argv: Vec<String>,
    pub cwd: String,
    pub pid: Option<u32>,
}

impl ProcessInfo {
    /// Basename of argv[0], e.g. "codex".
    pub fn bin_name(&self) -> &str {
        self.argv
            .first()
            .and_then(|a| a.rsplit('/').next())
            .unwrap_or("")
    }
}

/// A semantic event delivered to an adapter (hook payload, already parsed).
#[derive(Debug, Clone)]
pub struct AdapterEvent {
    /// Agent name as passed to `hook-event` ("codex", "claude", ...).
    pub agent: String,
    /// Hook/event name ("Stop", "PermissionRequest", "session.idle", ...).
    pub hook: String,
    /// Raw hook payload (fields vary per agent; unknown ignored).
    pub payload: Value,
}

impl AdapterEvent {
    pub fn payload_str(&self, key: &str) -> Option<String> {
        self.payload
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
}

/// Adapter's classification of one event. `None` fields = no change.
#[derive(Debug, Clone, Default)]
pub struct LifecycleDecision {
    pub lifecycle: Option<Lifecycle>,
    /// Some(Attention::None) explicitly clears (e.g. user replied).
    pub attention: Option<Attention>,
    /// Sidebar preview override.
    pub message: Option<String>,
}

/// Explicit notification the adapter wants created for this event.
#[derive(Debug, Clone)]
pub struct NotificationDraft {
    pub title: String,
    pub body: Option<String>,
    pub severity: NotificationSeverity,
}

/// Official resume command for a native session id.
#[derive(Debug, Clone)]
pub struct ResumeCommand {
    pub argv: Vec<String>,
}

/// How the server delivers a user's decision pick to the agent.
/// Exactly one variant until more channels are probed against real
/// CLIs (see `docs/adr/0008-decision-answer.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerChannel {
    /// Type the 1-based option number + Enter over `pane.input`.
    /// First probe: Codex TUI (numbered options).
    TypeText,
}

/// Bytes to write for `option_id`, or `None` when the id is unknown.
/// Pure: the headless-tested seam for answer delivery.
pub fn answer_bytes(
    channel: AnswerChannel,
    options: &[signaltty_core::model::DecisionOption],
    option_id: &str,
) -> Option<Vec<u8>> {
    match channel {
        AnswerChannel::TypeText => options
            .iter()
            .position(|o| o.id == option_id)
            .map(|i| format!("{}\n", i + 1).into_bytes()),
    }
}

#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    pub kind: AgentKind,
    pub display_name: &'static str,
    /// Binary basenames this adapter owns (for process detection).
    pub binaries: &'static [&'static str],
}

pub trait AgentAdapter: Send + Sync {
    /// Does this adapter own the pane's foreground process?
    fn identify(&self, proc: &ProcessInfo) -> bool;
    /// Map a semantic event to lifecycle + attention changes.
    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision;
    /// Native session id, if the event reports one.
    fn session_identity(&self, ev: &AdapterEvent) -> Option<String>;
    /// Map the event to an explicit notification (or none).
    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft>;
    /// Official resume argv for a persisted session id, if supported.
    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand>;
    /// How a picked decision option reaches this agent, if probed.
    /// Default `None`: read-only rendering until a channel is proven.
    fn answer_channel(&self) -> Option<AnswerChannel> {
        None
    }
    /// Static metadata.
    fn metadata(&self) -> AdapterMetadata;
}

#[cfg(test)]
mod tests {
    use super::*;
    use signaltty_core::model::DecisionOption;

    fn options() -> Vec<DecisionOption> {
        vec![
            DecisionOption {
                id: "once".into(),
                label: "Once".into(),
            },
            DecisionOption {
                id: "always".into(),
                label: "Always".into(),
            },
            DecisionOption {
                id: "deny".into(),
                label: "Deny".into(),
            },
        ]
    }

    #[test]
    fn type_text_answers_by_1_based_position() {
        let opts = options();
        assert_eq!(
            answer_bytes(AnswerChannel::TypeText, &opts, "once"),
            Some(b"1\n".to_vec())
        );
        assert_eq!(
            answer_bytes(AnswerChannel::TypeText, &opts, "deny"),
            Some(b"3\n".to_vec())
        );
        assert_eq!(answer_bytes(AnswerChannel::TypeText, &opts, "nope"), None);
    }
}
