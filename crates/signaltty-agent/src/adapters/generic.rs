//! Generic terminal fallback: owns everything the specific adapters
//! don't. Never changes lifecycle/attention from hook events (unknown
//! agents must keep working as normal terminals); title/BEL/exit/OSC
//! semantics live in the server core, not here.

use signaltty_core::model::AgentKind;

use crate::types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, LifecycleDecision, NotificationDraft, ProcessInfo,
    ResumeCommand,
};

pub struct GenericAdapter;

impl AgentAdapter for GenericAdapter {
    fn identify(&self, _proc: &ProcessInfo) -> bool {
        true // fallback: matches when nothing else does
    }

    fn lifecycle_state(&self, _ev: &AdapterEvent) -> LifecycleDecision {
        LifecycleDecision::default()
    }

    fn session_identity(&self, ev: &AdapterEvent) -> Option<String> {
        // Accept an explicitly reported id, nothing inferred.
        ev.payload_str("session_id")
    }

    fn notification_event(&self, _ev: &AdapterEvent) -> Option<NotificationDraft> {
        None
    }

    fn resume_capability(&self, _session_id: &str) -> Option<ResumeCommand> {
        None
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            kind: AgentKind::Generic,
            display_name: "Terminal",
            binaries: &[],
        }
    }
}
