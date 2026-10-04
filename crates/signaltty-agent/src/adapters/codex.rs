//! Codex CLI adapter.
//!
//! Hooks (`~/.codex/hooks.json`, Claude-shaped schema): SessionStart,
//! UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, Stop,
//! SessionEnd, SubagentStop, PreCompact, PostCompact. Payload on stdin
//! carries session_id, hook_event_name, cwd, turn_id, model,
//! last_assistant_message. No Notification event — `notify = [...]` in
//! config.toml fires on agent-turn-complete instead.

use signaltty_core::model::{AgentKind, NotificationSeverity};
use signaltty_core::state::{Attention, Lifecycle};

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct CodexAdapter;

impl AgentAdapter for CodexAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        matches!(proc.bin_name(), "codex")
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
                // The human just replied: clear any outstanding attention.
                attention: Some(Attention::None),
                message: None,
            },
            "PreToolUse" | "PostToolUse" | "PreCompact" | "PostCompact" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Working),
                attention: None,
                message: None,
            },
            "PermissionRequest" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Blocked),
                attention: Some(Attention::PermissionRequired),
                message: Some("approval requested".to_string()),
            },
            "Stop" | "agent-turn-complete" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("turn complete".to_string()),
            },
            "SessionEnd" => LifecycleDecision {
                lifecycle: Some(Lifecycle::Done),
                attention: Some(Attention::Unread),
                message: Some("session ended".to_string()),
            },
            // Subagent activity doesn't move the main pane's state.
            _ => LifecycleDecision::default(),
        }
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        ev.payload_str("session_id")
    }

    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft> {
        match ev.hook.as_str() {
            "PermissionRequest" => Some(NotificationDraft {
                title: "Codex needs approval".to_string(),
                body: ev
                    .payload_str("tool_name")
                    .or_else(|| ev.payload_str("description")),
                severity: NotificationSeverity::Warning,
            }),
            "Stop" | "agent-turn-complete" => Some(NotificationDraft {
                title: "Codex turn complete".to_string(),
                body: ev.payload_str("last_assistant_message"),
                severity: NotificationSeverity::Info,
            }),
            _ => None,
        }
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            argv: vec![
                "codex".to_string(),
                "resume".to_string(),
                session_id.to_string(),
            ],
        })
    }

    /// First probed channel: the Codex TUI offers numbered options,
    /// so typing the option number + Enter answers (fixture-verified
    /// byte delivery; live-TUI confirmation is follow-up work).
    fn answer_channel(&self) -> Option<crate::types::AnswerChannel> {
        Some(crate::types::AnswerChannel::TypeText)
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Codex,
            display_name: "Codex",
            binaries: &["codex"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(hook: &str, payload: serde_json::Value) -> AdapterEvent {
        AdapterEvent {
            agent: "codex".to_string(),
            hook: hook.to_string(),
            payload,
        }
    }

    #[test]
    fn permission_notification_names_the_tool() {
        let n = CodexAdapter
            .notification_event(&ev(
                "PermissionRequest",
                json!({"session_id": "s", "tool_name": "Bash", "tool_input": {}}),
            ))
            .unwrap();
        assert_eq!(n.body.as_deref(), Some("Bash"));
    }

    #[test]
    fn stop_means_done_unread_with_session() {
        let a = CodexAdapter;
        let e = ev("Stop", json!({"session_id": "sess-1"}));
        let d = a.lifecycle_state(&e);
        assert_eq!(d.lifecycle, Some(Lifecycle::Done));
        assert_eq!(d.attention, Some(Attention::Unread));
        assert_eq!(a.session_identity(&e), Some("sess-1".to_string()));
        let n = a.notification_event(&e).unwrap();
        assert_eq!(n.severity, NotificationSeverity::Info);
    }

    #[test]
    fn permission_request_blocks_hard() {
        let a = CodexAdapter;
        let d = a.lifecycle_state(&ev("PermissionRequest", json!({})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Blocked));
        assert_eq!(d.attention, Some(Attention::PermissionRequired));
    }

    #[test]
    fn prompt_submit_clears_attention() {
        let a = CodexAdapter;
        let d = a.lifecycle_state(&ev("UserPromptSubmit", json!({})));
        assert_eq!(d.lifecycle, Some(Lifecycle::Working));
        assert_eq!(d.attention, Some(Attention::None));
    }

    #[test]
    fn codex_advertises_type_text_channel() {
        let a = CodexAdapter;
        assert_eq!(
            a.answer_channel(),
            Some(crate::types::AnswerChannel::TypeText)
        );
    }

    #[test]
    fn resume_argv() {
        let a = CodexAdapter;
        let r = a.resume_capability("abc").unwrap();
        assert_eq!(r.argv, vec!["codex", "resume", "abc"]);
    }

    #[test]
    fn identify() {
        let a = CodexAdapter;
        assert!(a.identify(&ProcessInfo {
            argv: vec!["/usr/bin/codex".to_string()],
            cwd: "/tmp".to_string(),
            pid: None
        }));
        assert!(!a.identify(&ProcessInfo {
            argv: vec!["claude".to_string()],
            cwd: "/tmp".to_string(),
            pid: None
        }));
    }
    #[test]
    fn tool_use_and_subagent_hooks_do_not_clear_blocked_or_mutate_lifecycle() {
        let a = CodexAdapter;
        for hook in [
            "PreToolUse",
            "PostToolUse",
            "PreCompact",
            "PostCompact",
            "SubagentStop",
        ] {
            let d = a.lifecycle_state(&ev(hook, json!({})));
            assert_eq!(d.lifecycle, None, "{hook} must not change lifecycle");
        }
    }
}
