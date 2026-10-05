//! Agent adapters: semantic classification of hook events, session
//! identity extraction, and official resume commands.
//! Pure classifiers over snapshots — no PTY, no async, no OS.
//! See docs/07.

pub mod adapters;
pub mod manifest;
pub mod permission;
pub mod registry;
pub mod types;

pub use adapters::{claude, codex, cursor, generic, opencode, pi};
pub use manifest::{detect_kind_with_overlays, parse_manifest, Manifest, OverlayAdapter};
pub use registry::{adapter_for_kind, adapter_for_name, all_adapters, detect_kind};
pub use types::{
    AdapterEvent, AdapterMetadata, AgentAdapter, AnswerChannel, LifecycleDecision,
    NotificationDraft, ProcessInfo, ResumeCommand,
};

/// Re-exported for byte-delivery call sites.
pub use types::answer_bytes;
