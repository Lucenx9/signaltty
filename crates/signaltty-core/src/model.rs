//! Server → Workspace → Tab → Pane model. See docs/02.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::state::{Attention, Lifecycle};

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
    Generic,
    #[default]
    None,
}

impl AgentKind {
    pub fn parse(s: &str) -> Option<AgentKind> {
        Some(match s {
            "codex" => AgentKind::Codex,
            "claude" => AgentKind::Claude,
            "opencode" => AgentKind::Opencode,
            "cursor" => AgentKind::Cursor,
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
    pub attention: Attention,
    /// Latest explicit notification / hook summary. Never a raw scrape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl Pane {
    /// Attention clears only on explicit per-pane interaction.
    pub fn mark_seen(&mut self, now: DateTime<Utc>) {
        self.attention = Attention::None;
        self.last_seen_at = Some(now);
    }

    pub fn raise_attention(&mut self, next: Attention) {
        self.attention = self.attention.raise(next);
    }

    pub fn set_lifecycle(&mut self, next: Lifecycle) {
        if self.lifecycle != next {
            self.last_lifecycle = self.lifecycle;
            self.lifecycle = next;
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
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

#[cfg(test)]
mod tests {
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

    /// Width share of each pane left-to-right, for Right runs.
    fn widths(l: &Layout) -> Vec<f32> {
        fn walk(l: &Layout, scale: f32, out: &mut Vec<f32>) {
            match l {
                Layout::Pane { .. } => out.push(scale),
                Layout::Split { ratio, first, second, .. } => {
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
        let mut p = Pane {
            id: "pane_x".into(),
            workspace_id: "ws_x".into(),
            tab_id: "tab_x".into(),
            title: "t".into(),
            cwd: "/tmp".into(),
            argv: vec!["sh".into()],
            pty_size: PtySize::default(),
            live: LiveState::Live,
            restore_state: RestoreState::Live,
            agent: AgentInfo::default(),
            lifecycle: Lifecycle::Done,
            last_lifecycle: Lifecycle::Working,
            attention: Attention::Unread,
            last_message: None,
            created_at: now,
            last_activity_at: now,
            last_seen_at: None,
        };
        p.mark_seen(now);
        assert_eq!(p.attention, Attention::None);
        assert_eq!(p.lifecycle, Lifecycle::Done);
        assert!(p.last_seen_at.is_some());
    }
}
