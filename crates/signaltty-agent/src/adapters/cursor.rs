//! Cursor Agent adapter.
//!
//! Hooks (~/.cursor/hooks.json, {"version":1,"hooks":{...}}):
//! sessionStart (stable session_id, but also fires in Cursor Desktop —
//! the install recipe targets the CLI via SIGNALTTY_PANE presence),
//! beforeSubmitPrompt, preToolUse, postToolUse, stop, sessionEnd.
//! Payload on stdin carries hook_event_name and session fields.

use signaltty_core::model::{AgentKind, NotificationSeverity};
use signaltty_core::state::{Attention, Lifecycle};

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct CursorAdapter;

impl AgentAdapter for CursorAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        proc.bin_name() == "cursor-agent"
    }

    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision {
        match ev.hook.as_str() {
            "sessionStart" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Idle),
                attention: None,
                message: Some("session started".to_string()),
            },
            "beforeSubmitPrompt" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Working),
                attention: Some(Attention::None),
                message: None,
            },
            "preToolUse" | "postToolUse" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Working),
                attention: None,
                message: None,
            },
            "stop" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("turn complete".to_string()),
            },
            "sessionEnd" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("session ended".to_string()),
            },
            _ => LifecycleDecision::default(),
        }
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        ev.payload_str("session_id")
            .or_else(|| ev.payload_str("sessionId"))
            .or_else(|| ev.payload_str("conversation_id"))
            .or_else(|| ev.payload_str("chatId"))
    }

    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft> {
        match ev.hook.as_str() {
            "stop" => Some(NotificationDraft {
                title: "Cursor turn complete".to_string(),
                body: None,
                severity: NotificationSeverity::Info,
            }),
            _ => None,
        }
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            argv: vec![
                "cursor-agent".to_string(),
                "--resume".to_string(),
                session_id.to_string(),
            ],
        })
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Cursor,
            display_name: "Cursor Agent",
            binaries: &["cursor-agent"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(hook: &str, payload: serde_json::Value) -> AdapterEvent {
        AdapterEvent {
            agent: "cursor".to_string(),
            hook: hook.to_string(),
            payload,
        }
    }

    #[test]
    fn session_start_reports_identity() {
        let a = CursorAdapter;
        let e = ev("sessionStart", json!({"session_id": "chat-1"}));
        assert_eq!(a.session_identity(&e), Some("chat-1".to_string()));
        let d = a.lifecycle_state(&e);
        assert_eq!(d.lifecycle, Some(Lifecycle::Idle));
    }

    #[test]
    fn stop_completes() {
        let a = CursorAdapter;
        let d = a.lifecycle_state(&ev("stop", json!({})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Done));
        assert_eq!(d.attention, Some(Attention::Unread));
    }

    #[test]
    fn resume_argv() {
        let a = CursorAdapter;
        assert_eq!(
            a.resume_capability("abc").unwrap().argv,
            vec!["cursor-agent", "--resume", "abc"]
        );
    }
}
