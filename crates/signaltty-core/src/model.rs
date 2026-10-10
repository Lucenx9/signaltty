//! Server → Workspace → Tab → Pane model. See docs/02.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::state::{Attention, Lifecycle, TaskState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtySize {
    pub cols: u16,
    pub rows: u16,
}

impl PtySize {
    pub const MIN_COLS: u16 = 20;
    pub const MAX_COLS: u16 = 500;
    pub const MIN_ROWS: u16 = 5;
    pub const MAX_ROWS: u16 = 200;

    pub fn new(cols: u16, rows: u16) -> Result<PtySize, CoreError> {
        if !(Self::MIN_COLS..=Self::MAX_COLS).contains(&cols)
            || !(Self::MIN_ROWS..=Self::MAX_ROWS).contains(&rows)
        {
            return Err(CoreError::BadSize(format!("{cols}x{rows} out of range")));
        }
        Ok(PtySize { cols, rows })
    }

    /// Clamp client-requested sizes into range (resize arbitration).
    pub fn clamp(cols: u16, rows: u16) -> PtySize {
        PtySize {
            cols: cols.clamp(Self::MIN_COLS, Self::MAX_COLS),
            rows: rows.clamp(Self::MIN_ROWS, Self::MAX_ROWS),
        }
    }
}

impl Default for PtySize {
    fn default() -> Self {
        PtySize { cols: 80, rows: 24 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDir {
    Right,
    Down,
}

/// Binary split tree. The server stores it; the GUI renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Layout {
    Pane {
        pane_id: String,
    },
    Split {
        dir: SplitDir,
        ratio: f32,
        first: Box<Layout>,
        second: Box<Layout>,
    },
}

impl Layout {
    /// Hard stops for divider ratios: a client can never collapse a
    /// pane entirely, whatever pixels it reports.
    pub const MIN_RATIO: f32 = 0.05;
    pub const MAX_RATIO: f32 = 0.95;

    /// Clamp a divider ratio into range. Callers must reject
    /// non-finite input first (`clamp` panics on NaN).
    pub fn clamp_ratio(ratio: f32) -> f32 {
        ratio.clamp(Self::MIN_RATIO, Self::MAX_RATIO)
    }

    /// Validate leaf uniqueness and finite ratios, then normalize dividers.
    pub fn validate_and_normalize(&mut self) -> Result<(), CoreError> {
        fn validate(
            layout: &Layout,
            seen: &mut std::collections::HashSet<String>,
        ) -> Result<(), CoreError> {
            match layout {
                Layout::Pane { pane_id } => {
                    if !seen.insert(pane_id.clone()) {
                        return Err(CoreError::BadLayout(format!("duplicate pane {pane_id}")));
                    }
                }
                Layout::Split {
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    if !ratio.is_finite() {
                        return Err(CoreError::BadLayout("ratio must be finite".into()));
                    }
                    validate(first, seen)?;
                    validate(second, seen)?;
                }
            }
            Ok(())
        }
        fn normalize(layout: &mut Layout) {
            if let Layout::Split {
                ratio,
                first,
                second,
                ..
            } = layout
            {
                *ratio = Layout::clamp_ratio(*ratio);
                normalize(first);
                normalize(second);
            }
        }
        validate(self, &mut std::collections::HashSet::new())?;
        normalize(self);
        Ok(())
    }

    /// Replace the leaf holding `pane_id` with a split containing the old
    /// pane and `new_pane_id`, then divide the run evenly: every visual
    /// sibling in the same direction ends up with the same width (or
    /// height). Returns false if `pane_id` was not found.
    pub fn split(&mut self, pane_id: &str, dir: SplitDir, new_pane_id: String) -> bool {
        let inserted = self.split_at(pane_id, dir, new_pane_id.clone());
        if inserted {
            self.equalize_run(&new_pane_id, dir);
        }
        inserted
    }

    fn split_at(&mut self, pane_id: &str, dir: SplitDir, new_pane_id: String) -> bool {
        match self {
            Layout::Pane { pane_id: id } if id == pane_id => {
                let old = std::mem::replace(
                    self,
                    Layout::Pane {
                        pane_id: String::new(),
                    },
                );
                *self = Layout::Split {
                    dir,
                    ratio: 0.5,
                    first: Box::new(old),
                    second: Box::new(Layout::Pane {
                        pane_id: new_pane_id,
                    }),
                };
                true
            }
            Layout::Pane { .. } => false,
            Layout::Split { first, second, .. } => {
                first.split_at(pane_id, dir, new_pane_id.clone())
                    || second.split_at(pane_id, dir, new_pane_id)
            }
        }
    }

    /// Recompute the ratios on `pane_id`'s run in `dir` so every
    /// column (or row) gets an equal share. The run is the maximal
    /// subtree around the pane whose splits all point in `dir`;
    /// ancestors above it and nested cross-direction splits keep
    /// their ratios. Dragged dividers in the run reset to even.
    fn equalize_run(&mut self, pane_id: &str, dir: SplitDir) {
        let mut path = Vec::new();
        if !self.path_to(pane_id, &mut path) {
            return;
        }
        // The segment starts below the last cross-direction split on
        // the path; without one it is the whole tree.
        let mut depth = 0;
        let mut node = &*self;
        for (i, &side) in path.iter().enumerate() {
            match node {
                Layout::Split {
                    dir: d,
                    first,
                    second,
                    ..
                } => {
                    if *d != dir {
                        depth = i + 1;
                    }
                    node = if side { second } else { first };
                }
                Layout::Pane { .. } => break,
            }
        }
        let mut seg = &mut *self;
        for &side in &path[..depth] {
            match seg {
                Layout::Split { first, second, .. } => {
                    seg = if side { second } else { first };
                }
                Layout::Pane { .. } => return,
            }
        }
        seg.assign_equal(dir);
    }

    /// Rewrite every `dir` ratio in this subtree for equal shares.
    /// Returns the subtree's width in columns (a cross-direction
    /// subtree counts as one column, whatever it holds).
    fn assign_equal(&mut self, dir: SplitDir) -> f32 {
        match self {
            Layout::Split {
                dir: d,
                ratio,
                first,
                second,
            } if *d == dir => {
                let w_first = first.assign_equal(dir);
                let w_second = second.assign_equal(dir);
                *ratio = w_first / (w_first + w_second);
                w_first + w_second
            }
            _ => 1.0,
        }
    }

    /// Child choices from the root to `pane_id` (`false` = first,
    /// `true` = second). False if the pane is not in the tree.
    fn path_to(&self, pane_id: &str, path: &mut Vec<bool>) -> bool {
        match self {
            Layout::Pane { pane_id: id } => id == pane_id,
            Layout::Split { first, second, .. } => {
                path.push(false);
                if first.path_to(pane_id, path) {
                    return true;
                }
                path.pop();
                path.push(true);
                if second.path_to(pane_id, path) {
                    return true;
                }
                path.pop();
                false
            }
        }
    }

    fn node_at(&self, path: &[bool]) -> Option<&Layout> {
        let mut node = self;
        for &side in path {
            match node {
                Layout::Split { first, second, .. } => {
                    node = if side { second } else { first };
                }
                Layout::Pane { .. } => return None,
            }
        }
        Some(node)
    }

    fn node_at_mut(&mut self, path: &[bool]) -> Option<&mut Layout> {
        let mut node = self;
        for &side in path {
            match node {
                Layout::Split { first, second, .. } => {
                    node = if side { second } else { first };
                }
                Layout::Pane { .. } => return None,
            }
        }
        Some(node)
    }

    /// Set the ratio of the split at `path` (empty = root). False
    /// unless the path resolves to a `Split`.
    pub fn set_ratio_at_path(&mut self, path: &[bool], ratio: f32) -> bool {
        match self.node_at_mut(path) {
            Some(Layout::Split { ratio: r, .. }) => {
                *r = ratio;
                true
            }
            _ => false,
        }
    }

    /// Ratio of the split at `path`, if it resolves to a `Split`.
    pub fn ratio_at_path(&self, path: &[bool]) -> Option<f32> {
        match self.node_at(path) {
            Some(Layout::Split { ratio, .. }) => Some(*ratio),
            _ => None,
        }
    }

    /// True when `path` resolves to a `Split` (clients use it to tell
    /// a stale drag from a failed send).
    pub fn has_split_at(&self, path: &[bool]) -> bool {
        self.ratio_at_path(path).is_some()
    }

    /// Same tree ignoring split ratios: panes, directions and nesting
    /// must match. The GUI reconciles widget trees on this so a
    /// dragged divider never rebuilds a terminal.
    pub fn same_structure(&self, other: &Layout) -> bool {
        match (self, other) {
            (Layout::Pane { pane_id: a }, Layout::Pane { pane_id: b }) => a == b,
            (
                Layout::Split {
                    dir: da,
                    first: fa,
                    second: sa,
                    ..
                },
                Layout::Split {
                    dir: db,
                    first: fb,
                    second: sb,
                    ..
                },
            ) => da == db && fa.same_structure(fb) && sa.same_structure(sb),
            _ => false,
        }
    }

    /// Remove the leaf holding `pane_id`, collapsing its parent split.
    /// Returns false if `pane_id` was not found.
    pub fn remove(&mut self, pane_id: &str) -> bool {
        match self {
            Layout::Pane { pane_id: id } => id == pane_id,
            Layout::Split { first, second, .. } => {
                if matches!(**first, Layout::Pane { pane_id: ref id } if id == pane_id)
                    || matches!(**second, Layout::Pane { pane_id: ref id } if id == pane_id)
                {
                    // Collapse: keep the surviving sibling.
                    let survivor = if matches!(**first, Layout::Pane { pane_id: ref id } if id == pane_id)
                    {
                        std::mem::replace(
                            second.as_mut(),
                            Layout::Pane {
                                pane_id: String::new(),
                            },
                        )
                    } else {
                        std::mem::replace(
                            first.as_mut(),
                            Layout::Pane {
                                pane_id: String::new(),
                            },
                        )
                    };
                    *self = survivor;
                    true
                } else {
                    first.remove(pane_id) || second.remove(pane_id)
                }
            }
        }
    }

    /// Exchange the leaves `a` and `b`, keeping the tree shape and ratios.
    /// A tree holding only one of them renames it, so swapping across
    /// two tabs is the same call on both trees.
    pub fn swap_panes(&mut self, a: &str, b: &str) {
        match self {
            Layout::Pane { pane_id } if pane_id == a => *pane_id = b.to_string(),
            Layout::Pane { pane_id } if pane_id == b => *pane_id = a.to_string(),
            Layout::Pane { .. } => {}
            Layout::Split { first, second, .. } => {
                first.swap_panes(a, b);
                second.swap_panes(a, b);
            }
        }
    }

    pub fn panes(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<String>) {
        match self {
            Layout::Pane { pane_id } => out.push(pane_id.clone()),
            Layout::Split { first, second, .. } => {
                first.collect(out);
                second.collect(out);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Codex,
    Claude,
    Opencode,
    Cursor,
    Pi,
    Generic,
    #[default]
    None,
}

impl AgentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Codex => "codex",
            AgentKind::Claude => "claude",
            AgentKind::Opencode => "opencode",
            AgentKind::Cursor => "cursor",
            AgentKind::Pi => "pi",
            AgentKind::Generic => "generic",
            AgentKind::None => "none",
        }
    }

    pub fn parse(s: &str) -> Option<AgentKind> {
        Some(match s {
            "codex" => AgentKind::Codex,
            "claude" => AgentKind::Claude,
            "opencode" => AgentKind::Opencode,
            "cursor" => AgentKind::Cursor,
            "pi" => AgentKind::Pi,
            "generic" => AgentKind::Generic,
            "none" => AgentKind::None,
            _ => return None,
        })
    }
}

/// Adapter-owned native session identity. Persisted; never scraped from
/// terminal text while a better channel exists. See docs/07, docs/09.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInfo {
    pub kind: AgentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    /// Official adapter-built resume argv. Never auto-run without opt-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Non-secret provider config directories retained for an official resume.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub config_env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LiveState {
    Live,
    Exited { code: Option<i32> },
}

/// Honest post-restart presence. See docs/09.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RestoreState {
    Live,
    Restored,
    Resumable,
    Exited,
}

/// One answerable option of a pending [`Decision`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionOption {
    pub id: String,
    pub label: String,
}

/// A structured decision request from an agent (directive 2): rendered as
/// data-driven buttons, never scraped from terminal text. At most one per
/// pane; newer supersedes older. See `docs/adr/0008-decision-answer.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
    /// Captured at ingest from the adapter's answer channel: buttons iff
    /// true, else a read-only prompt with an answer-in-terminal hint.
    /// Defaults to true for forward-compat payloads predating channels.
    #[serde(default = "crate::model::decision_answerable_default")]
    pub answerable: bool,
    pub received_at: DateTime<Utc>,
}

fn decision_answerable_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pane {
    pub id: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub title: String,
    pub cwd: String,
    pub argv: Vec<String>,
    pub pty_size: PtySize,
    pub live: LiveState,
    pub restore_state: RestoreState,
    pub agent: AgentInfo,
    pub lifecycle: Lifecycle,
    pub last_lifecycle: Lifecycle,
    /// When `lifecycle` last changed; drives "Working for 2m…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_since: Option<DateTime<Utc>>,
    /// Length of the most recent `working` stretch, in seconds;
    /// drives "Worked for 2m" once the turn ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_secs: Option<i64>,
    pub attention: Attention,
    /// When `attention` was last raised; drives "Approval 10m".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention_since: Option<DateTime<Utc>>,
    /// Latest explicit notification / hook summary. Never a raw scrape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    /// Pending structured decision (directive 2). None = plain terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_decision: Option<Decision>,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Lineage & orchestration tracking (directive 018).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<Relationship>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

impl Pane {
    /// A freshly spawned, live pane titled after its command.
    pub fn new(
        workspace_id: String,
        tab_id: String,
        cwd: String,
        argv: Vec<String>,
        pty_size: PtySize,
        now: DateTime<Utc>,
    ) -> Pane {
        let title = argv
            .first()
            .map(|a| {
                std::path::Path::new(a)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| a.clone())
            })
            .unwrap_or_else(|| "shell".to_string());
        Pane {
            id: crate::ids::new_pane_id(),
            workspace_id,
            tab_id,
            title,
            cwd,
            argv,
            pty_size,
            live: LiveState::Live,
            restore_state: RestoreState::Live,
            agent: AgentInfo::default(),
            lifecycle: Lifecycle::Unknown,
            last_lifecycle: Lifecycle::Unknown,
            lifecycle_since: Some(now),
            last_run_secs: None,
            attention: Attention::None,
            attention_since: None,
            last_message: None,
            pending_decision: None,
            created_at: now,
            last_activity_at: now,
            last_seen_at: None,
            parent_pane_id: None,
            root_pane_id: None,
            label: None,
            relationship: None,
            task_id: None,
        }
    }

    /// Attention clears only on explicit per-pane interaction.
    pub fn mark_seen(&mut self, now: DateTime<Utc>) {
        self.attention = Attention::None;
        self.attention_since = None;
        self.last_seen_at = Some(now);
    }

    pub fn raise_attention(&mut self, next: Attention) {
        self.attention = self.attention.raise(next);
    }

    /// Leaving `working` records how long the run took.
    pub fn set_lifecycle(&mut self, next: Lifecycle, now: DateTime<Utc>) {
        if self.lifecycle == next {
            return;
        }
        if self.lifecycle == Lifecycle::Working {
            if let Some(since) = self.lifecycle_since {
                let secs = (now - since).num_seconds().max(0);
                self.last_run_secs = Some(secs);
            }
        }
        self.last_lifecycle = self.lifecycle;
        self.lifecycle = next;
        self.lifecycle_since = Some(now);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: String,
    pub workspace_id: String,
    pub title: String,
    /// None = empty tab (no panes yet).
    pub layout: Option<Layout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default)]
    pub detached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
}

/// Slugify a workspace name into a stable handle: lowercase alphanumerics
/// joined by single `-`, truncated to 32 chars, `ws` fallback. Never
/// contains `_`, so handles and `ws_…` ids stay disjoint by construction.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
        if out.len() >= 32 {
            break;
        }
    }
    let slug = out.trim_matches('-').to_string();
    if slug.is_empty() {
        "ws".to_string()
    } else {
        slug
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// Immutable unique handle (`my-api`): the stable typable address.
    /// `name` is the editable label. Backfilled on legacy snapshots.
    #[serde(default)]
    pub handle: String,
    pub cwd: String,
    #[serde(default)]
    pub git: GitInfo,
    pub tabs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_id: Option<String>,
    #[serde(default)]
    pub auto_resume: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Error,
}

impl NotificationSeverity {
    pub fn parse(s: &str) -> Option<NotificationSeverity> {
        Some(match s {
            "info" => NotificationSeverity::Info,
            "warning" => NotificationSeverity::Warning,
            "error" => NotificationSeverity::Error,
            _ => return None,
        })
    }

    pub fn attention(self) -> Attention {
        match self {
            NotificationSeverity::Info => Attention::Unread,
            NotificationSeverity::Warning => Attention::Warning,
            NotificationSeverity::Error => Attention::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub severity: NotificationSeverity,
    pub source: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<DateTime<Utc>>,
}

/// Relationship between parent and child tasks/panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Relationship {
    Fork,
    #[default]
    Subagent,
}

/// Task objective and requirements contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    pub objective: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_criteria: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
}

impl Contract {
    pub const MIN_OBJECTIVE_BYTES: usize = 1;
    pub const MAX_OBJECTIVE_BYTES: usize = 32 * 1024; // 32 KiB

    pub fn new(objective: impl Into<String>) -> Result<Self, CoreError> {
        let contract = Self {
            objective: objective.into(),
            constraints: None,
            acceptance_criteria: None,
            output_format: None,
        };
        contract.validate()?;
        Ok(contract)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.objective.len() < Self::MIN_OBJECTIVE_BYTES {
            return Err(CoreError::InvalidContract(
                "objective must not be empty".to_string(),
            ));
        }
        if self.objective.len() > Self::MAX_OBJECTIVE_BYTES {
            return Err(CoreError::InvalidContract(format!(
                "objective length {} exceeds max 32 KiB",
                self.objective.len()
            )));
        }
        Ok(())
    }
}

/// Artifact produced by a task worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Reported outcome status for a task result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskResultStatus {
    Completed,
    Failed,
    Rejected,
}

impl TaskResultStatus {
    pub fn to_state(self) -> TaskState {
        match self {
            TaskResultStatus::Completed => TaskState::Completed,
            TaskResultStatus::Failed => TaskState::Failed,
            TaskResultStatus::Rejected => TaskState::Rejected,
        }
    }
}

/// Structured result reported by a task worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    pub status: TaskResultStatus,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<serde_json::Value>,
    pub reported_at: DateTime<Utc>,
}

impl TaskResult {
    pub const MIN_SUMMARY_BYTES: usize = 1;
    pub const MAX_SUMMARY_BYTES: usize = 8 * 1024; // 8 KiB
    pub const MAX_ARTIFACTS: usize = 32;

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.summary.len() < Self::MIN_SUMMARY_BYTES {
            return Err(CoreError::InvalidTaskResult(
                "summary must not be empty".to_string(),
            ));
        }
        if self.summary.len() > Self::MAX_SUMMARY_BYTES {
            return Err(CoreError::InvalidTaskResult(format!(
                "summary length {} exceeds max 8 KiB",
                self.summary.len()
            )));
        }
        if self.artifacts.len() > Self::MAX_ARTIFACTS {
            return Err(CoreError::InvalidTaskResult(format!(
                "artifacts count {} exceeds max 32",
                self.artifacts.len()
            )));
        }
        Ok(())
    }
}

/// Final outcome of a completed/failed task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DispositionOutcome {
    #[default]
    None,
    Merged,
    Discarded,
}

/// Recorded finish disposition of a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disposition {
    pub outcome: DispositionOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_deleted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<DateTime<Utc>>,
}

impl Default for Disposition {
    fn default() -> Self {
        Self {
            outcome: DispositionOutcome::None,
            target_ref: None,
            merged_sha: None,
            branch_deleted: None,
            at: None,
        }
    }
}

/// Pull request state on the forge (e.g. GitHub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    #[default]
    Open,
    Merged,
    Closed,
}

impl PrState {
    pub fn parse_gh(s: &str) -> Option<Self> {
        match s.trim().to_uppercase().as_str() {
            "OPEN" => Some(Self::Open),
            "MERGED" => Some(Self::Merged),
            "CLOSED" => Some(Self::Closed),
            _ => None,
        }
    }
}

/// Rollup status of CI checks on a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrChecks {
    #[default]
    None,
    Pending,
    Passing,
    Failing,
}

/// Review decision status on a pull request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrReview {
    #[default]
    None,
    ReviewRequired,
    Approved,
    ChangesRequested,
}

/// Forge pull request associated with an orchestrated task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPr {
    pub number: u64,
    pub url: String,
    #[serde(default)]
    pub state: PrState,
    #[serde(default)]
    pub checks: PrChecks,
    #[serde(default)]
    pub review: PrReview,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<DateTime<Utc>>,
}

/// Parse a GitHub GraphQL reviewDecision string into `PrReview`.
pub fn parse_review_decision(s: &str) -> PrReview {
    match s.trim().to_uppercase().as_str() {
        "REVIEW_REQUIRED" => PrReview::ReviewRequired,
        "APPROVED" => PrReview::Approved,
        "CHANGES_REQUESTED" => PrReview::ChangesRequested,
        _ => PrReview::None,
    }
}

/// Roll up GitHub `statusCheckRollup` items into a single `PrChecks` status.
///
/// Priority 1: Failing (any failure, error, cancelled, timed_out, action_required, startup_failure)
/// Priority 2: Pending (any status != COMPLETED or state in PENDING, EXPECTED)
/// Priority 3: Passing (non-empty items and none of the above)
/// Priority 4: None (empty items)
pub fn rollup_status_checks(checks: &[serde_json::Value]) -> PrChecks {
    if checks.is_empty() {
        return PrChecks::None;
    }

    const FAILING_CONCLUSIONS: &[&str] = &[
        "FAILURE",
        "ERROR",
        "CANCELLED",
        "TIMED_OUT",
        "ACTION_REQUIRED",
        "STARTUP_FAILURE",
    ];
    const FAILING_STATES: &[&str] = &["FAILURE", "ERROR"];
    const PENDING_STATES: &[&str] = &["PENDING", "EXPECTED"];

    let mut has_pending = false;

    for item in checks {
        let conclusion = item
            .get("conclusion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_uppercase());
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| s.to_uppercase());
        let state = item
            .get("state")
            .and_then(|v| v.as_str())
            .map(|s| s.to_uppercase());

        if let Some(ref c) = conclusion {
            if FAILING_CONCLUSIONS.contains(&c.as_str()) {
                return PrChecks::Failing;
            }
        }
        if let Some(ref st) = state {
            if FAILING_STATES.contains(&st.as_str()) {
                return PrChecks::Failing;
            }
        }

        if let Some(ref st) = status {
            if st != "COMPLETED" {
                has_pending = true;
            }
        }
        if let Some(ref st) = state {
            if PENDING_STATES.contains(&st.as_str()) {
                has_pending = true;
            }
        }
    }

    if has_pending {
        PrChecks::Pending
    } else {
        PrChecks::Passing
    }
}

/// An orchestrated sub-task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub context_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pane_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_pane_id: Option<String>,
    #[serde(default)]
    pub relationship: Relationship,
    pub label: String,
    pub contract: Contract,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub source_repo: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_branch: Option<String>,
    pub worktree_path: PathBuf,
    pub branch: String,
    #[serde(default)]
    pub preexisting_branch: bool,
    pub base_ref: String,
    pub base_sha: String,
    pub state: TaskState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TaskResult>,
    #[serde(default)]
    pub disposition: Disposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<TaskPr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_error: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_cmd: Option<Vec<String>>,
    /// Caller-chosen idempotency key for `task.start`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Compose the submitted prompt from the task contract and metadata.
///
/// Pure function combining a fixed worker preamble, objective, optional constraints,
/// bulleted acceptance criteria, and expected output format.
pub fn compose_worker_prompt(
    task_id: &str,
    branch: &str,
    base_sha: &str,
    contract: &Contract,
) -> String {
    let mut out = format!(
        "You are a worker for task {task_id} in an isolated worktree on branch {branch} from base {base_sha}; stay inside this worktree; commit your work; when done or blocked run signaltty report --status completed|failed|rejected --summary … with evidence — it reads $SIGNALTTY_TASK.\n\n## Objective\n{}\n",
        contract.objective.trim()
    );

    if let Some(constraints) = &contract.constraints {
        let trimmed = constraints.trim();
        if !trimmed.is_empty() {
            out.push_str("\n## Constraints\n");
            out.push_str(trimmed);
            out.push('\n');
        }
    }

    if let Some(criteria) = &contract.acceptance_criteria {
        if !criteria.is_empty() {
            out.push_str("\n## Acceptance Criteria\n");
            for item in criteria {
                out.push_str(&format!("- {}\n", item.trim()));
            }
        }
    }

    if let Some(output_format) = &contract.output_format {
        let trimmed = output_format.trim();
        if !trimmed.is_empty() {
            out.push_str("\n## Expected Output Format\n");
            out.push_str(trimmed);
            out.push('\n');
        }
    }

    out
}

impl Task {
    pub fn compose_worker_prompt(&self) -> String {
        compose_worker_prompt(&self.id, &self.branch, &self.base_sha, &self.contract)
    }

    pub fn transition_to(&mut self, next: TaskState, now: DateTime<Utc>) -> Result<(), CoreError> {
        if !self.state.can_transition_to(next) {
            return Err(CoreError::InvalidTaskTransition(self.state, next));
        }
        self.state = next;
        self.updated_at = now;
        Ok(())
    }

    pub fn apply_report(
        &mut self,
        result: TaskResult,
        now: DateTime<Utc>,
    ) -> Result<(), CoreError> {
        result.validate()?;
        let target_state = result.status.to_state();
        if self.state.is_terminal() {
            return Err(CoreError::InvalidTaskTransition(self.state, target_state));
        }
        if !self.state.can_transition_to(target_state) {
            return Err(CoreError::InvalidTaskTransition(self.state, target_state));
        }
        self.state = target_state;
        self.result = Some(result);
        self.updated_at = now;
        Ok(())
    }

    pub fn apply_turn_ended_without_report(
        &mut self,
        evidence: Option<serde_json::Value>,
        now: DateTime<Utc>,
    ) -> bool {
        if self.state.is_terminal() {
            // Report wins over turn end race
            return false;
        }
        if self.state == TaskState::Working {
            self.state = TaskState::InputRequired;
            self.status_reason = evidence;
            self.updated_at = now;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn layout_validation_rejects_non_finite_ratios_without_mutation() {
        use super::{Layout, SplitDir};
        for ratio in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut layout = Layout::Split {
                dir: SplitDir::Right,
                ratio,
                first: Box::new(Layout::Pane {
                    pane_id: "a".into(),
                }),
                second: Box::new(Layout::Pane {
                    pane_id: "b".into(),
                }),
            };
            assert!(layout.validate_and_normalize().is_err());
            assert_eq!(
                layout.ratio_at_path(&[]).unwrap().to_bits(),
                ratio.to_bits()
            );
        }
    }

    use super::*;

    fn pane(id: &str) -> Layout {
        Layout::Pane {
            pane_id: id.to_string(),
        }
    }

    #[test]
    fn layout_split_and_remove() {
        let mut l = pane("a");
        assert!(l.split("a", SplitDir::Right, "b".into()));
        assert_eq!(l.panes(), vec!["a".to_string(), "b".to_string()]);
        assert!(l.split("b", SplitDir::Down, "c".into()));
        assert_eq!(
            l.panes(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert!(!l.split("zzz", SplitDir::Right, "d".into()));
        assert!(l.remove("b"));
        assert_eq!(l.panes(), vec!["a".to_string(), "c".to_string()]);
        assert!(!l.remove("zzz"));
    }

    #[test]
    fn swap_panes_exchanges_leaves_in_place() {
        let mut l = pane("a");
        l.split("a", SplitDir::Right, "b".into());
        l.split("b", SplitDir::Down, "c".into());
        let before = l.clone();
        l.swap_panes("a", "c");
        assert_eq!(l.panes(), vec!["c", "b", "a"]);
        l.swap_panes("a", "c");
        assert_eq!(l, before);
        // One side only: a rename, as in a cross-tab swap.
        l.swap_panes("b", "x");
        assert_eq!(l.panes(), vec!["a", "x", "c"]);
    }

    /// Width share of each pane left-to-right, for Right runs.
    fn widths(l: &Layout) -> Vec<f32> {
        fn walk(l: &Layout, scale: f32, out: &mut Vec<f32>) {
            match l {
                Layout::Pane { .. } => out.push(scale),
                Layout::Split {
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    walk(first, scale * ratio, out);
                    walk(second, scale * (1.0 - ratio), out);
                }
            }
        }
        let mut out = Vec::new();
        walk(l, 1.0, &mut out);
        out
    }

    fn assert_widths(l: &Layout, expected: &[f32]) {
        let got = widths(l);
        assert_eq!(got.len(), expected.len(), "widths {got:?}");
        for (g, e) in got.iter().zip(expected) {
            assert!((g - e).abs() < 1e-6, "widths {got:?} != {expected:?}");
        }
    }

    #[test]
    fn split_equalizes_same_direction_run() {
        // Repeatedly splitting the newest pane keeps every column equal.
        let mut l = pane("a");
        assert!(l.split("a", SplitDir::Right, "b".into()));
        assert_widths(&l, &[0.5, 0.5]);
        assert!(l.split("b", SplitDir::Right, "c".into()));
        assert_widths(&l, &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]);
        assert!(l.split("c", SplitDir::Right, "d".into()));
        assert_widths(&l, &[0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn split_equalizes_middle_of_run() {
        // Splitting a middle column equalizes the whole run, not just
        // the new pair.
        let mut l = pane("a");
        l.split("a", SplitDir::Right, "b".into());
        l.split("b", SplitDir::Right, "c".into());
        assert!(l.split("a", SplitDir::Right, "d".into()));
        assert_widths(&l, &[0.25, 0.25, 0.25, 0.25]);
        assert_eq!(l.panes(), vec!["a", "d", "b", "c"]);
    }

    #[test]
    fn split_leaves_other_direction_ratios_alone() {
        let mut l = pane("a");
        l.split("a", SplitDir::Right, "b".into());
        l.split("b", SplitDir::Right, "c".into());
        assert!(l.split("c", SplitDir::Down, "d".into()));
        // The Right run still divides in thirds; the Down pair in halves.
        assert_eq!(l.ratio_at_path(&[]), Some(1.0 / 3.0));
        assert_eq!(l.ratio_at_path(&[true]), Some(0.5));
        assert_eq!(l.ratio_at_path(&[true, true]), Some(0.5));
    }

    #[test]
    fn split_resets_dragged_ratios_in_run() {
        let mut l = pane("a");
        l.split("a", SplitDir::Right, "b".into());
        assert!(l.set_ratio_at_path(&[], 0.9));
        assert!(l.split("b", SplitDir::Right, "c".into()));
        assert_widths(&l, &[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0]);
    }

    #[test]
    fn split_equalizes_nested_same_direction_columns() {
        // Side subtrees with their own same-direction splits count all
        // their columns: [X Y] P -> [X Y] [P Q] in quarters.
        let mut l = Layout::Split {
            dir: SplitDir::Right,
            ratio: 0.5,
            first: Box::new(Layout::Split {
                dir: SplitDir::Right,
                ratio: 0.5,
                first: Box::new(pane("x")),
                second: Box::new(pane("y")),
            }),
            second: Box::new(pane("p")),
        };
        assert!(l.split("p", SplitDir::Right, "q".into()));
        assert_widths(&l, &[0.25, 0.25, 0.25, 0.25]);
    }

    #[test]
    fn ratio_at_path_resolves_splits_only() {
        let mut l = pane("a");
        l.split("a", SplitDir::Right, "b".into());
        assert_eq!(l.ratio_at_path(&[]), Some(0.5));
        assert_eq!(l.ratio_at_path(&[false]), None); // leaf, not a split
        assert_eq!(l.ratio_at_path(&[true, false]), None); // past a leaf
        assert!(l.set_ratio_at_path(&[], 0.25));
        assert_eq!(l.ratio_at_path(&[]), Some(0.25));
        assert!(!l.set_ratio_at_path(&[false], 0.25));
        assert!(!l.set_ratio_at_path(&[true, true], 0.25));
        assert!(l.has_split_at(&[]));
        assert!(!l.has_split_at(&[true]));
    }

    #[test]
    fn clamp_ratio_keeps_dividers_sane() {
        assert_eq!(Layout::clamp_ratio(0.5), 0.5);
        assert_eq!(Layout::clamp_ratio(0.0), 0.05);
        assert_eq!(Layout::clamp_ratio(1.0), 0.95);
        assert_eq!(Layout::clamp_ratio(-3.0), 0.05);
    }

    #[test]
    fn same_structure_ignores_ratios_only() {
        let mut a = pane("a");
        a.split("a", SplitDir::Right, "b".into());
        let mut b = a.clone();
        assert!(a.same_structure(&b));
        b.set_ratio_at_path(&[], 0.9);
        assert!(a.same_structure(&b));
        b.split("b", SplitDir::Down, "c".into());
        assert!(!a.same_structure(&b));
    }

    #[test]
    fn pty_size_clamp_and_validate() {
        assert!(PtySize::new(80, 24).is_ok());
        assert!(PtySize::new(1, 24).is_err());
        let s = PtySize::clamp(1000, 1);
        assert_eq!((s.cols, s.rows), (PtySize::MAX_COLS, PtySize::MIN_ROWS));
    }

    #[test]
    fn pane_mark_seen_keeps_lifecycle() {
        let now = Utc::now();
        let mut p = Pane::new(
            "ws_x".into(),
            "tab_x".into(),
            "/tmp".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        p.lifecycle = Lifecycle::Done;
        p.attention = Attention::Unread;
        p.mark_seen(now);
        assert_eq!(p.attention, Attention::None);
        assert_eq!(p.lifecycle, Lifecycle::Done);
        assert!(p.last_seen_at.is_some());
    }

    #[test]
    fn pane_new_titles_after_command() {
        let now = Utc::now();
        let p = Pane::new(
            "w".into(),
            "t".into(),
            "/".into(),
            vec!["/usr/bin/claude".into()],
            PtySize::default(),
            now,
        );
        assert_eq!(p.title, "claude");
        let p = Pane::new(
            "w".into(),
            "t".into(),
            "/".into(),
            vec![],
            PtySize::default(),
            now,
        );
        assert_eq!(p.title, "shell");
    }

    #[test]
    fn leaving_working_records_run_length() {
        let t0 = Utc::now();
        let mut p = Pane::new(
            "w".into(),
            "t".into(),
            "/".into(),
            vec!["sh".into()],
            PtySize::default(),
            t0,
        );
        p.set_lifecycle(Lifecycle::Working, t0);
        p.set_lifecycle(Lifecycle::Working, t0 + chrono::Duration::seconds(30));
        assert_eq!(
            p.lifecycle_since,
            Some(t0),
            "no-op transition keeps the clock"
        );
        p.set_lifecycle(Lifecycle::Done, t0 + chrono::Duration::seconds(125));
        assert_eq!(p.last_run_secs, Some(125));
        assert_eq!(p.last_lifecycle, Lifecycle::Working);
        p.set_lifecycle(Lifecycle::Idle, t0 + chrono::Duration::seconds(200));
        assert_eq!(p.last_run_secs, Some(125), "only working stretches count");
    }

    #[test]
    fn slugify_makes_stable_typable_handles() {
        assert_eq!(slugify("My API!!"), "my-api");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("!!!"), "ws");
        assert_eq!(slugify(""), "ws");
        assert_eq!(slugify("UPPER_snake"), "upper-snake");
        let long = slugify(&"a".repeat(100));
        assert!(long.len() <= 32);
        for s in ["my-api", "ws", &long] {
            assert!(!s.contains('_'), "{s}");
            assert!(!s.starts_with('-') && !s.ends_with('-'), "{s}");
        }
    }

    #[test]
    fn decision_roundtrips_and_defaults_for_old_snapshots() {
        let now = Utc::now();
        let d = Decision {
            id: "d1".into(),
            prompt: "Allow rm -rf /tmp/x?".into(),
            options: vec![
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
            ],
            answerable: false,
            received_at: now,
        };
        let back: Decision = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
        assert_eq!(back, d);
        // Payloads predating channels stay answerable.
        let legacy: Decision = serde_json::from_value(serde_json::json!({
            "id": "d1", "prompt": "p", "options": [],
            "received_at": now,
        }))
        .unwrap();
        assert!(legacy.answerable);
        // Old pane snapshots load with no pending decision and default lineage fields.
        let p = Pane::new(
            "w".into(),
            "t".into(),
            "/".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        assert_eq!(p.pending_decision, None);
        let v = serde_json::to_value(&p).unwrap();
        assert!(
            v.get("pending_decision").is_none(),
            "None skips serialization"
        );
        let back: Pane = serde_json::from_value(serde_json::json!({
            "id": p.id, "workspace_id": "w", "tab_id": "t", "title": "sh",
            "cwd": "/", "argv": ["sh"], "pty_size": {"cols": 80, "rows": 24},
            "live": {"state": "live"}, "restore_state": "LIVE",
            "agent": {"kind": "none"}, "lifecycle": "unknown", "last_lifecycle": "unknown",
            "attention": "none", "created_at": now, "last_activity_at": now,
        }))
        .unwrap();
        assert_eq!(back.pending_decision, None);
        assert_eq!(back.parent_pane_id, None);
        assert_eq!(back.root_pane_id, None);
        assert_eq!(back.label, None);
        assert_eq!(back.relationship, None);
        assert_eq!(back.task_id, None);
    }

    #[test]
    fn contract_validation_bounds() {
        assert!(Contract::new("").is_err());
        assert!(Contract::new("x").is_ok());

        let max_str = "a".repeat(Contract::MAX_OBJECTIVE_BYTES);
        assert!(Contract::new(max_str).is_ok());

        let over_str = "a".repeat(Contract::MAX_OBJECTIVE_BYTES + 1);
        assert!(Contract::new(over_str).is_err());
    }

    #[test]
    fn task_result_validation_bounds() {
        let now = Utc::now();
        let res_empty = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "".to_string(),
            artifacts: vec![],
            evidence: None,
            reported_at: now,
        };
        assert!(res_empty.validate().is_err());

        let res_max_summary = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "s".repeat(TaskResult::MAX_SUMMARY_BYTES),
            artifacts: vec![],
            evidence: None,
            reported_at: now,
        };
        assert!(res_max_summary.validate().is_ok());

        let res_over_summary = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "s".repeat(TaskResult::MAX_SUMMARY_BYTES + 1),
            artifacts: vec![],
            evidence: None,
            reported_at: now,
        };
        assert!(res_over_summary.validate().is_err());

        let artifacts_ok: Vec<Artifact> = (0..32)
            .map(|i| Artifact {
                name: format!("art_{i}"),
                path: format!("path/{i}"),
                version: None,
            })
            .collect();
        let res_max_art = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "ok".to_string(),
            artifacts: artifacts_ok,
            evidence: None,
            reported_at: now,
        };
        assert!(res_max_art.validate().is_ok());

        let artifacts_over: Vec<Artifact> = (0..33)
            .map(|i| Artifact {
                name: format!("art_{i}"),
                path: format!("path/{i}"),
                version: None,
            })
            .collect();
        let res_over_art = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "ok".to_string(),
            artifacts: artifacts_over,
            evidence: None,
            reported_at: now,
        };
        assert!(res_over_art.validate().is_err());
    }

    #[test]
    fn task_lifecycle_and_report_wins() {
        let now = Utc::now();
        let contract = Contract::new("Do work").unwrap();
        let mut task = Task {
            id: crate::ids::new_task_id(),
            context_id: crate::ids::new_context_id(),
            parent_task_id: None,
            pane_id: Some("pane_1".to_string()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: "test-task".to_string(),
            contract,
            agent: Some("codex".to_string()),
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: Some("main".to_string()),
            worktree_path: PathBuf::from("/tmp/worktree"),
            branch: "task-branch".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "abcd1234abcd".to_string(),
            state: TaskState::Pending,
            result: None,
            disposition: Disposition::default(),
            pr: None,
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: now,
            updated_at: now,
        };

        // Pending -> Working
        assert!(task.transition_to(TaskState::Working, now).is_ok());
        assert_eq!(task.state, TaskState::Working);

        // Working -> turn ended without report -> InputRequired
        let reason = serde_json::json!({"reason": "turn_ended_without_report"});
        assert!(task.apply_turn_ended_without_report(Some(reason.clone()), now));
        assert_eq!(task.state, TaskState::InputRequired);
        assert_eq!(task.status_reason, Some(reason));

        // Follow-up: InputRequired -> Working
        assert!(task.transition_to(TaskState::Working, now).is_ok());
        assert_eq!(task.state, TaskState::Working);

        // Worker reports completed
        let result = TaskResult {
            status: TaskResultStatus::Completed,
            summary: "All done successfully".to_string(),
            artifacts: vec![],
            evidence: None,
            reported_at: now,
        };
        assert!(task.apply_report(result.clone(), now).is_ok());
        assert_eq!(task.state, TaskState::Completed);
        assert_eq!(task.result, Some(result));

        // Turn end arrived after report: report wins, turn end is ignored
        assert!(!task.apply_turn_ended_without_report(None, now));
        assert_eq!(task.state, TaskState::Completed);

        // Terminal state is immutable
        assert!(task.transition_to(TaskState::Working, now).is_err());
    }

    #[test]
    fn worker_prompt_composition_from_full_contract() {
        let contract = Contract {
            objective: "Implement feature X".to_string(),
            constraints: Some("Do not touch vendor folder".to_string()),
            acceptance_criteria: Some(vec![
                "All tests pass".to_string(),
                "Clippy clean".to_string(),
            ]),
            output_format: Some("JSON format output".to_string()),
        };

        let prompt = compose_worker_prompt("task_123", "feature-x", "abc456", &contract);

        assert!(prompt.contains("You are a worker for task task_123 in an isolated worktree on branch feature-x from base abc456;"));
        assert!(prompt.contains("stay inside this worktree; commit your work; when done or blocked run signaltty report --status completed|failed|rejected --summary … with evidence — it reads $SIGNALTTY_TASK"));
        assert!(prompt.contains("## Objective\nImplement feature X\n"));
        assert!(prompt.contains("## Constraints\nDo not touch vendor folder\n"));
        assert!(prompt.contains("## Acceptance Criteria\n- All tests pass\n- Clippy clean\n"));
        assert!(prompt.contains("## Expected Output Format\nJSON format output\n"));

        // Minimal contract without optional fields
        let minimal = Contract::new("Just the objective").unwrap();
        let min_prompt = compose_worker_prompt("task_999", "main", "base000", &minimal);
        assert!(min_prompt.contains("You are a worker for task task_999 in an isolated worktree on branch main from base base000"));
        assert!(min_prompt.contains("## Objective\nJust the objective\n"));
        assert!(!min_prompt.contains("## Constraints"));
        assert!(!min_prompt.contains("## Acceptance Criteria"));
        assert!(!min_prompt.contains("## Expected Output Format"));
    }

    #[test]
    fn test_parse_review_decision() {
        assert_eq!(
            parse_review_decision("REVIEW_REQUIRED"),
            PrReview::ReviewRequired
        );
        assert_eq!(
            parse_review_decision("review_required"),
            PrReview::ReviewRequired
        );
        assert_eq!(parse_review_decision("APPROVED"), PrReview::Approved);
        assert_eq!(parse_review_decision("approved"), PrReview::Approved);
        assert_eq!(
            parse_review_decision("CHANGES_REQUESTED"),
            PrReview::ChangesRequested
        );
        assert_eq!(
            parse_review_decision("changes_requested"),
            PrReview::ChangesRequested
        );
        assert_eq!(parse_review_decision(""), PrReview::None);
        assert_eq!(parse_review_decision("unknown"), PrReview::None);
    }

    #[test]
    fn test_pr_state_parse_gh() {
        assert_eq!(PrState::parse_gh("OPEN"), Some(PrState::Open));
        assert_eq!(PrState::parse_gh("open"), Some(PrState::Open));
        assert_eq!(PrState::parse_gh("MERGED"), Some(PrState::Merged));
        assert_eq!(PrState::parse_gh("merged"), Some(PrState::Merged));
        assert_eq!(PrState::parse_gh("CLOSED"), Some(PrState::Closed));
        assert_eq!(PrState::parse_gh("closed"), Some(PrState::Closed));
        assert_eq!(PrState::parse_gh("draft"), None);
    }

    #[test]
    fn test_rollup_status_checks() {
        use serde_json::json;

        // Empty checks -> None
        assert_eq!(rollup_status_checks(&[]), PrChecks::None);

        // All passing check runs
        let passing = vec![
            json!({"status": "COMPLETED", "conclusion": "SUCCESS"}),
            json!({"status": "COMPLETED", "conclusion": "NEUTRAL"}),
            json!({"state": "SUCCESS"}),
        ];
        assert_eq!(rollup_status_checks(&passing), PrChecks::Passing);

        // Pending check run (status != COMPLETED)
        let pending1 = vec![
            json!({"status": "COMPLETED", "conclusion": "SUCCESS"}),
            json!({"status": "IN_PROGRESS", "conclusion": null}),
        ];
        assert_eq!(rollup_status_checks(&pending1), PrChecks::Pending);

        // Pending status context (state == PENDING)
        let pending2 = vec![
            json!({"status": "COMPLETED", "conclusion": "SUCCESS"}),
            json!({"state": "PENDING"}),
        ];
        assert_eq!(rollup_status_checks(&pending2), PrChecks::Pending);

        // Failing overrides pending (Priority 1 > Priority 2)
        let failing1 = vec![
            json!({"status": "IN_PROGRESS"}),
            json!({"status": "COMPLETED", "conclusion": "FAILURE"}),
        ];
        assert_eq!(rollup_status_checks(&failing1), PrChecks::Failing);

        let failing2 = vec![
            json!({"state": "ERROR"}),
            json!({"status": "COMPLETED", "conclusion": "SUCCESS"}),
        ];
        assert_eq!(rollup_status_checks(&failing2), PrChecks::Failing);

        let failing_cancelled = vec![json!({"status": "COMPLETED", "conclusion": "TIMED_OUT"})];
        assert_eq!(rollup_status_checks(&failing_cancelled), PrChecks::Failing);
    }

    #[test]
    fn test_task_pr_serde_backward_compat() {
        use serde_json::json;
        // Task serialized without pr field deserializes with pr = None
        let task_json = json!({
            "id": "task_1",
            "context_id": "ctx_1",
            "label": "test",
            "contract": {"objective": "do something"},
            "source_repo": "/repo",
            "worktree_path": "/wt",
            "branch": "b",
            "base_ref": "main",
            "base_sha": "abc",
            "state": "pending",
            "created_at": "2026-10-04T12:00:00Z",
            "updated_at": "2026-10-04T12:00:00Z"
        });
        let task: Task = serde_json::from_value(task_json).unwrap();
        assert_eq!(task.pr, None);

        // Task with pr
        let pr = TaskPr {
            number: 42,
            url: "https://github.com/foo/bar/pull/42".to_string(),
            state: PrState::Open,
            checks: PrChecks::Passing,
            review: PrReview::Approved,
            checked_at: None,
        };
        let mut task_with_pr = task.clone();
        task_with_pr.pr = Some(pr);
        let val = serde_json::to_value(&task_with_pr).unwrap();
        assert_eq!(val["pr"]["number"], 42);
        assert_eq!(val["pr"]["state"], "open");
        assert_eq!(val["pr"]["checks"], "passing");
        assert_eq!(val["pr"]["review"], "approved");

        // Round trip
        let round_trip: Task = serde_json::from_value(val).unwrap();
        assert_eq!(round_trip.pr, task_with_pr.pr);
    }
}
