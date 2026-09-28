//! Claude Code adapter.
//!
//! Hooks (settings JSON: ~/.claude/settings.json, project/local,
//! --settings): SessionStart, UserPromptSubmit, PreToolUse,
//! PostToolUse, Stop, SubagentStop, Notification (idle_prompt and
//! permission prompts), PreCompact, SessionEnd. Payload session_id
//! equals $CLAUDE_CODE_SESSION_ID and survives -p --resume.

use signaltty_core::model::{AgentKind, NotificationSeverity};
use signaltty_core::state::{Attention, Lifecycle};

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct ClaudeAdapter;

impl AgentAdapter for ClaudeAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        matches!(proc.bin_name(), "claude")
    }

    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision {
        match ev.hook.as_str() {
            "SessionStart" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Idle),
                attention: None,
                message: Some("session started".to_string()),
            },
            "UserPromptSubmit" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Working),
                attention: Some(Attention::None),
                message: None,
            },
            "PreToolUse" | "PostToolUse" | "PreCompact" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Working),
                attention: None,
                message: None,
            },
            "Notification" => {
                // idle_prompt (agent waiting on user) vs permission prompt.
                let kind = ev.payload_str("notification_type").unwrap_or_default();
                if kind.contains("permission") {
                    LifecycleDecision {
                        lifecycle: Some(Lifecycle::Blocked),
                        attention: Some(Attention::PermissionRequired),
                        message: Some("permission requested".to_string()),
                    }
                } else {
                    LifecycleDecision {
                        lifecycle: Some(Lifecycle::Blocked),
                        attention: Some(Attention::InputRequired),
                        message: Some("input requested".to_string()),
                    }
                }
            }
            "Stop" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("turn complete".to_string()),
            },
            "SessionEnd" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("session ended".to_string()),
            },
            _ => LifecycleDecision::default(),
        }
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        ev.payload_str("session_id")
    }

    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft> {
        match ev.hook.as_str() {
            "Notification" => {
                let kind = ev.payload_str("notification_type").unwrap_or_default();
                let permission = kind.contains("permission");
                Some(NotificationDraft {
                    title: if permission {
                        "Claude needs approval".to_string()
                    } else {
                        "Claude needs input".to_string()
                    },
                    body: ev.payload_str("message"),
                    severity: if permission {
                        NotificationSeverity::Warning
                    } else {
                        NotificationSeverity::Info
                    },
                })
            }
            "Stop" => Some(NotificationDraft {
                title: "Claude turn complete".to_string(),
                body: ev.payload_str("last_assistant_message"),
                severity: NotificationSeverity::Info,
            }),
            _ => None,
        }
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            argv: vec![
                "claude".to_string(),
                "--resume".to_string(),
                session_id.to_string(),
            ],
        })
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Claude,
            display_name: "Claude Code",
            binaries: &["claude"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(hook: &str, payload: serde_json::Value) -> AdapterEvent {
        AdapterEvent {
            agent: "claude".to_string(),
            hook: hook.to_string(),
            payload,
        }
    }

    #[test]
    fn notification_idle_blocks_on_input() {
        let a = ClaudeAdapter;
        let e = ev(
            "Notification",
            json!({"notification_type": "idle_prompt", "message": "done?", "session_id": "s"}),
        );
        let d = a.lifecycle_state(&e);
        assert_eq!(d.lifecycle, Some(Lifecycle::Blocked));
        assert_eq!(d.attention, Some(Attention::InputRequired));
        let n = a.notification_event(&e).unwrap();
        assert_eq!(n.title, "Claude needs input");
        assert_eq!(n.body, Some("done?".to_string()));
    }

    #[test]
    fn notification_permission_is_stronger() {
        let a = ClaudeAdapter;
        let d = a.lifecycle_state(&ev(
            "Notification",
            json!({"notification_type": "permission_prompt"}),
        ));
        assert_eq!(d.attention, Some(Attention::PermissionRequired));
    }

    #[test]
    fn resume_argv() {
        let a = ClaudeAdapter;
        assert_eq!(
            a.resume_capability("abc").unwrap().argv,
            vec!["claude", "--resume", "abc"]
        );
    }
}
