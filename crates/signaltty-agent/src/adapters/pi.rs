//! Pi agent adapter (https://github.com/badlogic/pi-mono).
//!
//! Provides process detection (`pi`) and session resume (`pi --session <id>`).
//! Pi has no Signaltty hook integration yet; status relies on generic
//! terminal signals.

use signaltty_core::model::AgentKind;

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct PiAdapter;

impl AgentAdapter for PiAdapter {
    fn identify(&self, proc: &ProcessInfo) -> bool {
        proc.bin_name() == "pi"
    }

    fn lifecycle_state(&self, _ev: &AdapterEvent) -> LifecycleDecision {
        LifecycleDecision::default()
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        ev.payload_str("session_id")
    }

    fn notification_event(&self, _ev: &AdapterEvent) -> Option<NotificationDraft> {
        None
    }

    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            argv: vec![
                "pi".to_string(),
                "--session".to_string(),
                session_id.to_string(),
            ],
        })
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Pi,
            display_name: "Pi",
            binaries: &["pi"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identify() {
        let a = PiAdapter;
        assert!(a.identify(&ProcessInfo {
            argv: vec!["pi".to_string()],
            cwd: "/tmp".to_string(),
            pid: None,
        }));
        assert!(a.identify(&ProcessInfo {
            argv: vec!["/usr/local/bin/pi".to_string()],
            cwd: "/tmp".to_string(),
            pid: None,
        }));
        assert!(!a.identify(&ProcessInfo {
            argv: vec!["opencode".to_string()],
            cwd: "/tmp".to_string(),
            pid: None,
        }));
    }

    #[test]
    fn resume_argv() {
        let a = PiAdapter;
        assert_eq!(
            a.resume_capability("abc").unwrap().argv,
            vec!["pi", "--session", "abc"]
        );
    }
}
