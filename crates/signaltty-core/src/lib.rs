//! Domain model: workspaces, tabs, panes, lifecycle, attention.
//! Toolkit-free, async-free, OS-free. See docs/02 and docs/03.

pub mod diff;
pub mod error;
pub mod ids;
pub mod model;
pub mod paths;
pub mod state;

pub use error::CoreError;
pub use ids::{new_notif_id, new_pane_id, new_tab_id, new_ws_id};
pub use model::{
    AgentInfo, AgentKind, Decision, DecisionOption, GitInfo, Layout, LiveState, Notification,
    NotificationSeverity, Pane, PtySize, RestoreState, SplitDir, Tab, Workspace,
};
pub use state::{Attention, Lifecycle};
