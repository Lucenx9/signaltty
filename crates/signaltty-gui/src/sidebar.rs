//! Workspace sidebar: what each project's agents are doing and whether
//! one needs you, at a glance. Rows are keyed by workspace id and
//! updated in place — refreshes never rebuild them, so selection,
//! scroll position and status transitions survive every server event.
//!
//! Row anatomy (Mail-style, fixed three lines):
//!
//! ```text
//!  ◌  api-server                     2m     lifecycle · name · time
//!     Bash(cargo test -p api)   Approval    latest message · attention
//!     main · claude                         branch/dir · agents
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use gtk4::prelude::*;

use signaltty_core::{AgentKind, Attention, Lifecycle, Pane, Workspace};

use crate::status::{self, AttentionBadge, LifecycleIndicator};
use crate::util::{tilde, time_ago};

pub struct WsSummary {
    pub id: String,
    pub name: String,
    pub lifecycle: Lifecycle,
    pub attention: Attention,
    /// Latest explicit message, else a lifecycle description.
    pub message: String,
    /// "branch · agents" (directory when not a git repo).
    pub meta: String,
    pub last_activity: Option<DateTime<Utc>>,
}

/// Roll a workspace's panes up into one sidebar row (docs/03).
pub fn summarize(ws: &Workspace, panes: &[Pane]) -> WsSummary {
    let lifecycle = status::worst_lifecycle(panes);
    let attention = status::worst_attention(panes);
    let message = panes
        .iter()
        .filter(|p| p.last_message.as_deref().is_some_and(|m| !m.is_empty()))
        .max_by_key(|p| p.last_activity_at)
        .and_then(|p| p.last_message.clone())
        .unwrap_or_else(|| status::lifecycle_label(lifecycle).to_string());
    let mut agents: Vec<&str> = panes
        .iter()
        .filter_map(|p| agent_name(p.agent.kind))
        .collect();
    agents.sort_unstable();
    agents.dedup();
    let place = ws.git.branch.clone().unwrap_or_else(|| tilde(&ws.cwd));
    let meta = if agents.is_empty() {
        place
    } else {
        format!("{place} · {}", agents.join(", "))
    };
    WsSummary {
        id: ws.id.clone(),
        name: ws.name.clone(),
        lifecycle,
        attention,
        message,
        meta,
        last_activity: panes.iter().map(|p| p.last_activity_at).max(),
    }
}

/// Priority order for the sidebar (docs/14 §1): attention severity,
/// then lifecycle rank (blocked → done → working → idle), then
/// recency, then name so full ties stay deterministic across
/// refreshes instead of jittering.
pub fn sort_summaries(items: &mut [WsSummary]) {
    items.sort_by(|a, b| {
        b.attention
            .severity()
            .cmp(&a.attention.severity())
            .then(b.lifecycle.sidebar_rank().cmp(&a.lifecycle.sidebar_rank()))
            .then(b.last_activity.cmp(&a.last_activity))
            .then(a.name.cmp(&b.name))
    });
}

/// Display name for real agents; shells and unknown commands have none.
pub fn agent_name(kind: AgentKind) -> Option<&'static str> {
    match kind {
        AgentKind::Claude => Some("Claude"),
        AgentKind::Codex => Some("Codex"),
        AgentKind::Opencode => Some("opencode"),
        AgentKind::Cursor => Some("Cursor"),
        AgentKind::Generic | AgentKind::None => None,
    }
}

struct Row {
    id: String,
    row: gtk4::ListBoxRow,
    name: gtk4::Label,
    time: gtk4::Label,
    message: gtk4::Label,
    meta: gtk4::Label,
    lifecycle: LifecycleIndicator,
    badge: AttentionBadge,
    last_activity: Option<DateTime<Utc>>,
}

impl Row {
    fn new(id: &str) -> Row {
        let label = |classes: &[&str]| {
            let l = gtk4::Label::new(None);
            l.set_xalign(0.0);
            l.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            for c in classes {
                l.add_css_class(c);
            }
            l
        };
        let name = label(&["workspace-name"]);
        name.set_hexpand(true);
        let message = label(&["workspace-message"]);
        message.set_hexpand(true);
        let meta = label(&["caption", "dimmed"]);
        let time = gtk4::Label::new(None);
        time.add_css_class("caption");
        time.add_css_class("numeric");
        time.add_css_class("dimmed");
        time.set_halign(gtk4::Align::End);
        let lifecycle = LifecycleIndicator::new();
        let badge = AttentionBadge::new();
        badge.widget.set_halign(gtk4::Align::End);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(10);
        grid.set_row_spacing(2);
        grid.add_css_class("workspace-row");
        grid.attach(&lifecycle.widget, 0, 0, 1, 1);
        grid.attach(&name, 1, 0, 1, 1);
        grid.attach(&time, 2, 0, 1, 1);
        grid.attach(&message, 1, 1, 1, 1);
        grid.attach(&badge.widget, 2, 1, 1, 1);
        grid.attach(&meta, 1, 2, 2, 1);

        let row = gtk4::ListBoxRow::new();
        row.set_widget_name(id);
        row.set_child(Some(&grid));
        Row {
            id: id.to_string(),
            row,
            name,
            time,
            message,
            meta,
            lifecycle,
            badge,
            last_activity: None,
        }
    }

    fn update(&mut self, s: &WsSummary) {
        self.name.set_text(&s.name);
        self.message.set_text(&s.message);
        self.message.set_tooltip_text(Some(&s.message));
        self.meta.set_text(&s.meta);
        self.lifecycle.set(s.lifecycle);
        self.badge.set(s.attention);
        self.last_activity = s.last_activity;
        self.refresh_time();
    }

    fn refresh_time(&self) {
        self.time
            .set_text(&self.last_activity.map(time_ago).unwrap_or_default());
    }
}

type SelectCallback = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;

pub struct Sidebar {
    pub widget: gtk4::ScrolledWindow,
    list: gtk4::ListBox,
    rows: RefCell<Vec<Row>>,
    on_select: SelectCallback,
}

impl Sidebar {
    pub fn new() -> Sidebar {
        let list = gtk4::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let on_select: SelectCallback = Rc::new(RefCell::new(None));
        {
            let on_select = Rc::clone(&on_select);
            list.connect_row_selected(move |_, row| {
                if let (Some(row), Some(cb)) = (row, on_select.borrow().as_ref()) {
                    cb(row.widget_name().to_string());
                }
            });
        }
        let widget = gtk4::ScrolledWindow::new();
        widget.set_child(Some(&list));
        widget.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        widget.set_vexpand(true);
        Sidebar {
            widget,
            list,
            rows: RefCell::new(Vec::new()),
            on_select,
        }
    }

    pub fn set_on_select(&self, cb: impl Fn(String) + 'static) {
        *self.on_select.borrow_mut() = Some(Box::new(cb));
    }

    /// Reconcile rows with `items` (priority order), updating in
    /// place. Moves remove + re-insert rows; GTK keeps the selection
    /// on the moved row, so selection follows the workspace, not the
    /// row index (covered by the display test's order assertions).
    pub fn update(&self, items: &[WsSummary]) {
        let mut rows = self.rows.borrow_mut();
        rows.retain(|r| {
            let keep = items.iter().any(|s| s.id == r.id);
            if !keep {
                self.list.remove(&r.row);
            }
            keep
        });
        for (i, item) in items.iter().enumerate() {
            match rows.iter().position(|r| r.id == item.id) {
                Some(pos) if pos == i => rows[i].update(item),
                Some(pos) => {
                    let mut row = rows.remove(pos);
                    self.list.remove(&row.row);
                    self.list.insert(&row.row, i as i32);
                    row.update(item);
                    rows.insert(i, row);
                }
                None => {
                    let mut row = Row::new(&item.id);
                    row.update(item);
                    self.list.insert(&row.row, i as i32);
                    rows.insert(i, row);
                }
            }
        }
    }

    /// Re-render relative times ("now" → "2m") between server events.
    pub fn refresh_times(&self) {
        for r in self.rows.borrow().iter() {
            r.refresh_time();
        }
    }

    pub fn select(&self, ws_id: &str) {
        if let Some(r) = self.rows.borrow().iter().find(|r| r.id == ws_id) {
            if !r.row.is_selected() {
                self.list.select_row(Some(&r.row));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn summary(
        name: &str,
        lifecycle: Lifecycle,
        attention: Attention,
        minutes_ago: Option<i64>,
    ) -> WsSummary {
        WsSummary {
            id: name.to_string(),
            name: name.to_string(),
            lifecycle,
            attention,
            message: String::new(),
            meta: String::new(),
            last_activity: minutes_ago.map(|m| {
                Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap() - chrono::Duration::minutes(m)
            }),
        }
    }

    fn names(items: &[WsSummary]) -> Vec<&str> {
        items.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn attention_severity_beats_lifecycle() {
        let mut items = vec![
            summary(
                "blocked-quiet",
                Lifecycle::Blocked,
                Attention::None,
                Some(0),
            ),
            summary("idle-error", Lifecycle::Idle, Attention::Error, Some(60)),
            summary("idle-unread", Lifecycle::Idle, Attention::Unread, Some(0)),
        ];
        sort_summaries(&mut items);
        assert_eq!(
            names(&items),
            ["idle-error", "idle-unread", "blocked-quiet"]
        );
    }

    #[test]
    fn lifecycle_follows_directive_order_within_equal_attention() {
        let mut items = vec![
            summary("idle", Lifecycle::Idle, Attention::None, Some(0)),
            summary("working", Lifecycle::Working, Attention::None, Some(0)),
            summary("done", Lifecycle::Done, Attention::None, Some(0)),
            summary("blocked", Lifecycle::Blocked, Attention::None, Some(0)),
        ];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["blocked", "done", "working", "idle"]);
    }

    #[test]
    fn recency_then_name_break_ties_deterministically() {
        let mut items = vec![
            summary("b-old", Lifecycle::Idle, Attention::None, Some(30)),
            summary("a-new", Lifecycle::Idle, Attention::None, Some(1)),
            summary("c-new", Lifecycle::Idle, Attention::None, Some(1)),
            summary("d-never", Lifecycle::Idle, Attention::None, None),
        ];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["a-new", "c-new", "b-old", "d-never"]);
        let mut again = vec![
            summary("d-never", Lifecycle::Idle, Attention::None, None),
            summary("c-new", Lifecycle::Idle, Attention::None, Some(1)),
            summary("b-old", Lifecycle::Idle, Attention::None, Some(30)),
            summary("a-new", Lifecycle::Idle, Attention::None, Some(1)),
        ];
        sort_summaries(&mut again);
        assert_eq!(names(&again), names(&items), "order input-independent");
    }
}
