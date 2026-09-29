//! Workspace sidebar: what each project's agents are doing and whether
//! one needs you, at a glance. Rows are keyed by workspace id and
//! updated in place — refreshes never rebuild them, so selection,
//! scroll position and status transitions survive every server event.
//!
//! Row anatomy (t3code-style, fixed three lines, no leading column):
//!
//! ```text
//!  api-server                 ◌ Working     name · status slot (time when calm)
//!  Bash(cargo test -p api)                  headline
//!  feat/auth                     Claude     branch/dir · agents
//! ```
//!
//! The close button shares the status slot and crossfades in on hover
//! or keyboard focus, so a resting row carries no controls. When rows
//! need you, they sit under a "Needs you" label with a hairline below.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use gtk4::prelude::*;

use signaltty_core::{AgentKind, Attention, Lifecycle, Pane, Workspace};

use crate::status::{self, StatusSlot};
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
    /// Branch, or the directory when not a git repo.
    pub place: String,
    /// Agent display names, comma-joined; empty for plain shells.
    pub agents: String,
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
    WsSummary {
        id: ws.id.clone(),
        name: ws.name.clone(),
        lifecycle,
        attention,
        message,
        lifecycle_since: lead.and_then(|p| p.lifecycle_since),
        last_run_secs: lead.and_then(|p| p.last_run_secs),
        place: ws.git.branch.clone().unwrap_or_else(|| tilde(&ws.cwd)),
        agents: agents.join(", "),
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

/// Rows the human must act on (warning and up). Sorting puts them
/// first, so they form one contiguous "Needs you" section.
pub fn needs_you(a: Attention) -> bool {
    a.severity() >= Attention::Warning.severity()
}

#[derive(Debug, PartialEq, Eq)]
pub enum SectionHeader {
    /// Label above the first row that needs you.
    NeedsYou,
    /// Hairline between that section and the rest.
    Rest,
}

/// Header for a row given whether it and the row above need you. With
/// nothing waiting there are no headers at all.
pub fn section_header(above: Option<bool>, needs: bool) -> Option<SectionHeader> {
    match (above, needs) {
        (None | Some(false), true) => Some(SectionHeader::NeedsYou),
        (Some(true), false) => Some(SectionHeader::Rest),
        _ => None,
    }
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
    status: StatusSlot,
    message: gtk4::Label,
    place: gtk4::Label,
    agents: gtk4::Label,
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
        let place = label(&["workspace-meta"]);
        place.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        place.set_hexpand(true);
        let agents = label(&["workspace-meta"]);
        agents.set_halign(gtk4::Align::End);
        let status = StatusSlot::new();
        let close = gtk4::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.add_css_class("circular");
        close.add_css_class("row-close");
        close.set_halign(gtk4::Align::End);
        close.set_valign(gtk4::Align::Center);
        let close_id = id.to_string();
        let on_close = Rc::clone(on_close);
        close.connect_clicked(move |_| {
            if let Some(cb) = on_close.borrow().as_ref() {
                cb(close_id.clone());
            }
        });
        // Status and close share one slot; CSS swaps them on hover.
        let slot = gtk4::Overlay::new();
        slot.set_child(Some(&status.widget));
        slot.add_overlay(&close);
        slot.set_size_request(28, -1);
        slot.set_halign(gtk4::Align::End);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(8);
        grid.set_row_spacing(2);
        grid.add_css_class("workspace-row");
        grid.attach(&name, 0, 0, 1, 1);
        grid.attach(&slot, 1, 0, 1, 1);
        grid.attach(&message, 0, 1, 2, 1);
        grid.attach(&place, 0, 2, 1, 1);
        grid.attach(&agents, 1, 2, 1, 1);

        let row = gtk4::ListBoxRow::new();
        row.set_widget_name(id);
        row.set_child(Some(&grid));
        Row {
            id: id.to_string(),
            row,
            name,
            status,
            message,
            place,
            agents,
            close,
            summary: None,
        }
    }

    fn update(&mut self, s: WsSummary) {
        if needs_you(s.attention) {
            self.row.add_css_class(NEEDS_YOU);
        } else {
            self.row.remove_css_class(NEEDS_YOU);
        }
        self.name.set_text(&s.name);
        self.place.set_text(&s.place);
        self.place.set_tooltip_text(Some(&s.place));
        self.agents.set_text(&s.agents);
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
        let time = s.last_activity.map(time_ago).unwrap_or_default();
        self.status.set(s.lifecycle, s.attention, &time);
    }
}

/// Row class the section headers key on.
const NEEDS_YOU: &str = "needs-you";

fn apply_section_header(row: &gtk4::ListBoxRow, above: Option<&gtk4::ListBoxRow>) {
    let header = section_header(
        above.map(|r| r.has_css_class(NEEDS_YOU)),
        row.has_css_class(NEEDS_YOU),
    );
    let widget: Option<gtk4::Widget> = match header {
        None => None,
        Some(SectionHeader::NeedsYou) => {
            let label = gtk4::Label::new(Some("Needs you"));
            label.set_xalign(0.0);
            label.add_css_class("sidebar-section");
            Some(label.upcast())
        }
        Some(SectionHeader::Rest) => {
            let rule = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            rule.add_css_class("sidebar-section-rule");
            Some(rule.upcast())
        }
    };
    // Keep an equivalent header rather than churn widgets on refresh.
    let same = match (&widget, row.header()) {
        (None, None) => true,
        (Some(new), Some(old)) => new.type_() == old.type_(),
        _ => false,
    };
    if !same {
        row.set_header(widget.as_ref());
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
        list.set_header_func(apply_section_header);
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
        // Section membership can change without a move.
        self.list.invalidate_headers();
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
            place: String::new(),
            agents: String::new(),
            last_activity: minutes_ago.map(|m| {
                Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap() - chrono::Duration::minutes(m)
            }),
        }
    }

    fn names(items: &[WsSummary]) -> Vec<&str> {
        items.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn sections_appear_only_around_rows_that_need_you() {
        use SectionHeader::*;
        let headers = |needs: &[bool]| -> Vec<Option<SectionHeader>> {
            needs
                .iter()
                .enumerate()
                .map(|(i, n)| section_header(i.checked_sub(1).map(|j| needs[j]), *n))
                .collect()
        };
        assert_eq!(
            headers(&[true, true, false, false]),
            [Some(NeedsYou), None, Some(Rest), None]
        );
        assert_eq!(headers(&[false, false]), [None, None], "calm → no headers");
        assert_eq!(headers(&[true]), [Some(NeedsYou)]);
        assert!(needs_you(Attention::Warning));
        assert!(needs_you(Attention::PermissionRequired));
        assert!(!needs_you(Attention::Unread));
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
