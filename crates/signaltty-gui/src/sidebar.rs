//! Workspace sidebar: what each project's agents are doing and whether
//! one needs you, at a glance. Rows are keyed by workspace id and
//! updated in place — refreshes never rebuild them, so selection,
//! scroll position and status transitions survive every server event.
//!
//! Row anatomy (Mail-style, fixed three lines):
//!
//! ```text
//!  ◌  api-server                     2m     lifecycle · name · time
//!     Bash(cargo test -p api)   Approval    headline · attention
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
    /// Latest explicit message; the headline falls back to run state.
    pub message: Option<String>,
    /// Timing of the pane that sets `lifecycle` (see `headline`).
    pub lifecycle_since: Option<DateTime<Utc>>,
    pub last_run_secs: Option<i64>,
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
        .and_then(|p| p.last_message.clone());
    // The longest-running pane in the rolled-up state speaks for the
    // workspace: "Working for 12m…" beats a sibling's 1m. Untimed
    // panes (pre-timing snapshots) sort last — `None` would
    // otherwise win `min_by_key` and hide a timed sibling.
    let lead = panes
        .iter()
        .filter(|p| p.lifecycle == lifecycle)
        .min_by_key(|p| (p.lifecycle_since.is_none(), p.lifecycle_since));
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
        lifecycle_since: lead.and_then(|p| p.lifecycle_since),
        last_run_secs: lead.and_then(|p| p.last_run_secs),
        meta,
        last_activity: panes.iter().map(|p| p.last_activity_at).max(),
    }
}

impl WsSummary {
    /// Second-line text: explicit message, else the verb-tense run
    /// state, else the plain lifecycle word.
    pub fn headline(&self, now: DateTime<Utc>) -> String {
        self.message
            .clone()
            .or_else(|| {
                status::run_label(
                    self.lifecycle,
                    self.lifecycle_since,
                    self.last_run_secs,
                    now,
                )
            })
            .unwrap_or_else(|| status::lifecycle_label(self.lifecycle).to_string())
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
    close: gtk4::Button,
    /// Kept so the 30s tick can re-render time-derived text.
    summary: Option<WsSummary>,
}

impl Row {
    fn new(id: &str, on_close: &CloseCallback) -> Row {
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
        let close = gtk4::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_valign(gtk4::Align::Center);
        let close_id = id.to_string();
        let on_close = Rc::clone(on_close);
        close.connect_clicked(move |_| {
            if let Some(cb) = on_close.borrow().as_ref() {
                cb(close_id.clone());
            }
        });

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
        grid.attach(&close, 3, 0, 1, 3);

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
            close,
            summary: None,
        }
    }

    fn update(&mut self, s: WsSummary) {
        self.name.set_text(&s.name);
        self.meta.set_text(&s.meta);
        self.lifecycle.set(s.lifecycle);
        self.badge.set(s.attention);
        self.close
            .set_tooltip_text(Some(&format!("Close {}", s.name)));
        self.close
            .update_property(&[gtk4::accessible::Property::Label(&format!(
                "Close workspace {}",
                s.name
            ))]);
        self.summary = Some(s);
        self.refresh_time();
    }

    fn refresh_time(&self) {
        let Some(s) = &self.summary else { return };
        let headline = s.headline(Utc::now());
        if self.message.text() != headline {
            self.message.set_text(&headline);
            self.message.set_tooltip_text(Some(&headline));
        }
        self.time
            .set_text(&s.last_activity.map(time_ago).unwrap_or_default());
    }
}

type SelectCallback = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;
type CloseCallback = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;

pub struct Sidebar {
    pub widget: gtk4::ScrolledWindow,
    list: gtk4::ListBox,
    rows: RefCell<Vec<Row>>,
    on_select: SelectCallback,
    on_close: CloseCallback,
}

impl Sidebar {
    pub fn new() -> Sidebar {
        let list = gtk4::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let on_select: SelectCallback = Rc::new(RefCell::new(None));
        let on_close: CloseCallback = Rc::new(RefCell::new(None));
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
            on_close,
        }
    }

    pub fn set_on_select(&self, cb: impl Fn(String) + 'static) {
        *self.on_select.borrow_mut() = Some(Box::new(cb));
    }

    pub fn set_on_close(&self, cb: impl Fn(String) + 'static) {
        *self.on_close.borrow_mut() = Some(Box::new(cb));
    }

    /// Reconcile rows with `items` (priority order), updating in
    /// place. Moves remove + re-insert rows; GTK keeps the selection
    /// on the moved row, so selection follows the workspace, not the
    /// row index (covered by the display test's order assertions).
    pub fn update(&self, items: Vec<WsSummary>) {
        let mut rows = self.rows.borrow_mut();
        rows.retain(|r| {
            let keep = items.iter().any(|s| s.id == r.id);
            if !keep {
                self.list.remove(&r.row);
            }
            keep
        });
        for (i, item) in items.into_iter().enumerate() {
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
                    let mut row = Row::new(&item.id, &self.on_close);
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
    use signaltty_core::PtySize;

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
            message: None,
            lifecycle_since: None,
            last_run_secs: None,
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

    #[test]
    fn headline_prefers_message_then_run_state_then_lifecycle() {
        let now = Utc::now();
        let mut s = summary("ws", Lifecycle::Done, Attention::None, Some(0));
        assert_eq!(s.headline(now), "Done", "no timing → plain word");
        s.last_run_secs = Some(130);
        assert_eq!(s.headline(now), "Worked for 2m");
        s.message = Some("Bash(cargo test)".into());
        assert_eq!(s.headline(now), "Bash(cargo test)");
    }

    fn workspace() -> Workspace {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        Workspace {
            id: "ws".into(),
            name: "ws".into(),
            cwd: "/tmp".into(),
            git: Default::default(),
            tabs: vec![],
            active_tab_id: None,
            auto_resume: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn pane(lifecycle: Lifecycle, since: Option<DateTime<Utc>>) -> Pane {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let mut p = Pane::new(
            "ws".into(),
            "tab".into(),
            "/tmp".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        p.lifecycle = lifecycle;
        p.lifecycle_since = since;
        p
    }

    #[test]
    fn lead_pane_is_the_longest_running_of_the_rolled_up_state() {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let ago = |m| Some(t0 - chrono::Duration::minutes(m));
        let panes = vec![
            pane(Lifecycle::Working, ago(1)),
            pane(Lifecycle::Working, ago(12)),
            pane(Lifecycle::Idle, ago(60)),
        ];
        let s = summarize(&workspace(), &panes);
        assert_eq!(s.lifecycle, Lifecycle::Working);
        assert_eq!(s.lifecycle_since, ago(12));
        assert_eq!(s.headline(t0), "Working for 12m…");
    }

    #[test]
    fn untimed_legacy_pane_never_shadows_a_timed_sibling() {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let panes = vec![
            pane(Lifecycle::Working, None),
            pane(Lifecycle::Working, Some(t0 - chrono::Duration::minutes(12))),
        ];
        let s = summarize(&workspace(), &panes);
        assert_eq!(
            s.headline(t0),
            "Working for 12m…",
            "`None` timing must sort last, not win `min_by_key`"
        );
    }
}
