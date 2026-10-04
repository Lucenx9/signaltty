//! Domain model: workspaces, tabs, panes, lifecycle, attention.
//! Toolkit-free, async-free, OS-free. See docs/02 and docs/03.

pub mod diff;
pub mod error;
pub mod ids;
pub mod model;
pub mod paths;
pub mod state;
pub mod theme;

pub use error::CoreError;
pub use ids::{
    has_prefix, new_context_id, new_notif_id, new_pane_id, new_tab_id, new_task_id, new_ws_id,
};
pub use model::{
    compose_worker_prompt, AgentInfo, AgentKind, Artifact, Contract, Decision, DecisionOption,
    Disposition, DispositionOutcome, GitInfo, Layout, LiveState, Notification,
    NotificationSeverity, Pane, PtySize, Relationship, RestoreState, SplitDir, Tab, Task,
    TaskResult, TaskResultStatus, Workspace,
};
pub use state::{Attention, Lifecycle, TaskState};
pub use theme::{clamp_sidebar_width, Appearance, GuiPreference, Theme};
