//! OpenCode adapter.
//!
//! JS/TS plugin system (`~/.config/opencode/plugins/` or project
//! `.opencode/plugins/`). Our plugin normalizes to `hook-event` calls
//! (`session.created`, `session.status` with `status` busy/idle,
//! `session.idle`, `session.error`, each carrying `session_id`).
//! Raw plugin event fields (`properties.sessionID`) are handled in the
//! plugin itself; the adapter only sees normalized payloads.
//! V2 execution failures normalize to `session.error`, retaining the provider
//! message rather than reporting successful idle completion.

use signaltty_core::model::{AgentKind, NotificationSeverity};
use signaltty_core::state::{Attention, Lifecycle};

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct OpencodeAdapter;

impl AgentAdapter for OpencodeAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        matches!(proc.bin_name(), "opencode")
    }

    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision {
        match ev.hook.as_str() {
            "session.created" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Idle),
                attention: None,
                message: Some("session started".to_string()),
            },
            "session.status" => match ev.payload_str("status").as_deref() {
                Some("busy") => LifecycleDecision {
                    lifecycle: Some(Lifecycle::Working),
                    // A new turn started: drop the previous turn's attention.
                    attention: Some(Attention::None),
                    message: None,
                },
                Some("idle") => LifecycleDecision {
                    lifecycle: Some(Lifecycle::Done),
                    attention: Some(Attention::Unread),
                    message: Some("session idle".to_string()),
                },
                _ => LifecycleDecision::default(),
            },
            "session.idle" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("session idle".to_string()),
            },
            "session.error" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Failed),
                attention: Some(Attention::Error),
                message: Some("session error".to_string()),
            },
            _ => LifecycleDecision::default(),
        }
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        ev.payload_str("session_id")
            .or_else(|| ev.payload_str("sessionID"))
    }

    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft> {
        match ev.hook.as_str() {
            "session.idle" => Some(NotificationDraft {
                title: "OpenCode session idle".to_string(),
                body: None,
                severity: NotificationSeverity::Info,
            }),
            "session.error" => Some(NotificationDraft {
                title: "OpenCode session error".to_string(),
                body: ev.payload_str("message"),
                severity: NotificationSeverity::Error,
            }),
            _ => None,
        }
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            argv: vec![
                "opencode".to_string(),
                "--session".to_string(),
                session_id.to_string(),
            ],
        })
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Opencode,
            display_name: "OpenCode",
            binaries: &["opencode"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(hook: &str, payload: serde_json::Value) -> AdapterEvent {
        AdapterEvent {
            agent: "opencode".to_string(),
            hook: hook.to_string(),
            payload,
        }
    }

    #[test]
    fn status_busy_idle_cycle() {
        let a = OpencodeAdapter;
        let d = a.lifecycle_state(&ev("session.status", json!({"status": "busy"})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Working));
        assert_eq!(d.attention, Some(Attention::None));
        let d = a.lifecycle_state(&ev(
            "session.status",
            json!({"status": "idle", "session_id": "s"}),
        ));
        assert_eq!(d.lifecycle, Some(Lifecycle::Done));
        assert_eq!(d.attention, Some(Attention::Unread));
    }

    #[test]
    fn resume_argv() {
        let a = OpencodeAdapter;
        assert_eq!(
            a.resume_capability("abc").unwrap().argv,
            vec!["opencode", "--session", "abc"]
        );
    }
}
