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
    /// Replace the leaf holding `pane_id` with a split containing the old
    /// pane and `new_pane_id`. Returns false if `pane_id` was not found.
    pub fn split(&mut self, pane_id: &str, dir: SplitDir, new_pane_id: String) -> bool {
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
                first.split(pane_id, dir, new_pane_id.clone())
                    || second.split(pane_id, dir, new_pane_id)
            }
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
