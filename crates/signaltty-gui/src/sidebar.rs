//! Workspace sidebar: what each project's agents are doing and whether
//! one needs you, at a glance. Rows are keyed by workspace id and
//! updated in place — refreshes never rebuild them, so selection,
//! scroll position and status transitions survive every server event.
//!
//! Root row anatomy (t3code-style, three lines, mark sits inline before name):
//!
//! ```text
//!  [AS] api-server            ◴ Working     mark · name · status slot
//!  [Task Working] Bash…                     task chips, then headline
//!  ▾ 2 tasks  feat/auth       Claude        disclosure · branch/dir · agents
//! ```
//!
//! Child row anatomy (two compact lines, ~20px indent, mark & 3rd line hidden):
//!
//! ```text
//!       api-server-worker     ◴ Working     name · status slot
//!       [Task Working] Tests…               task chips, then headline
//! ```
//!
//! Finished child task rows (`workspace-finished`) recede with reduced opacity
//! until hovered or selected. Finished children sort after unfinished siblings
//! within their group.
//!
//! Close has a separate trailing target, revealed on hover or keyboard
//! focus without hiding urgency. Right-clicking or pressing Menu / Shift+F10
//! displays a context menu. When rows (or any child in a group) need you,
//! they sit under a "Needs you" label with a hairline below.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use gtk4::gio;
use gtk4::prelude::*;

use signaltty_core::{AgentKind, Attention, Lifecycle, Pane, Workspace};

use crate::status::{self, StatusSlot};
use crate::task_chip::{TaskChipView, TaskChipWidget};
use crate::util::{tilde, time_ago};

#[derive(Clone)]
pub struct WsSummary {
    pub id: String,
    pub name: String,
    /// Shown as `name · handle` only when sibling workspaces share the
    /// name (handles are unique by construction); `None` keeps rows quiet.
    pub disambiguator: Option<String>,
    /// Parent workspace id when this workspace is a task workspace nested
    /// under another workspace.
    pub parent: Option<String>,
    /// True when this task workspace has finished its turn or resolved.
    pub finished: bool,
    /// Set during sorting: true if this workspace or any member of its
    /// group needs human attention.
    pub group_needs_you: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub lifecycle: Lifecycle,
    pub attention: Attention,
    /// Latest explicit message; the headline falls back to run state.
    pub message: Option<String>,
    /// Timing of the pane that sets `lifecycle` (see `headline`).
    pub lifecycle_since: Option<DateTime<Utc>>,
    pub last_run_secs: Option<i64>,
    /// When the oldest pane at `attention` was raised to it.
    pub attention_since: Option<DateTime<Utc>>,
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
        .filter(|p| status::effective_lifecycle(p) == lifecycle)
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
        disambiguator: None,
        parent: None,
        finished: false,
        group_needs_you: needs_you(attention),
        created_at: ws.created_at,
        lifecycle,
        attention,
        message,
        lifecycle_since: lead.and_then(|p| p.lifecycle_since),
        attention_since: panes
            .iter()
            .filter(|p| p.attention == attention)
            .filter_map(|p| p.attention_since)
            .min(),
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

/// True when the headline only restates that a plain process runs.
fn quiet_headline(s: &WsSummary) -> bool {
    s.message.is_none() && s.lifecycle == Lifecycle::Unknown
}

/// Priority order for the sidebar (docs/14 §1): groups of root workspaces
/// and their child task workspaces. Groups sort by worst attention severity,
/// then best lifecycle rank (blocked → failed → done → working → idle),
/// then root workspace age (newer first), then root name.
/// Deliberately NOT last-activity: activity timestamps change on every agent event,
/// so a recency tiebreak reorders rows under the pointer constantly — clicks
/// land on the wrong row and the top row never looks settled (spec 001 amendment, 2026-09-29).
/// Fresh output still floats via `unread` severity; recency remains visible
/// as the relative time in the status slot.
///
/// Hierarchy rules:
/// - Tasks with parent panes define child workspaces.
/// - Parent chains flatten to ONE level (root workspace).
/// - Cycles terminate and orphan children become roots.
/// - If any member of a group needs you, the whole group carries `group_needs_you`.
/// - Children follow their root immediately, sorted among themselves by the same key.
pub fn sort_summaries(items: &mut [WsSummary]) {
    if items.is_empty() {
        return;
    }

    // 1. Flatten parent chains to ONE level (root), terminating cycles and
    // demoting orphans whose parent is not present in `items` to roots.
    let known_ids: std::collections::HashSet<String> = items.iter().map(|s| s.id.clone()).collect();
    let parent_map: std::collections::HashMap<String, String> = items
        .iter()
        .filter_map(|s| s.parent.as_ref().map(|p| (s.id.clone(), p.clone())))
        .collect();

    for item in items.iter_mut() {
        if let Some(initial_parent) = &item.parent {
            let mut curr = initial_parent.clone();
            let mut visited = std::collections::HashSet::new();
            visited.insert(item.id.clone());
            let mut root = None;

            while visited.insert(curr.clone()) {
                if !known_ids.contains(&curr) {
                    break;
                }
                if let Some(next_parent) = parent_map.get(&curr) {
                    curr = next_parent.clone();
                } else {
                    root = Some(curr);
                    break;
                }
            }
            item.parent = root;
        }
    }

    // 2. Separate roots and index children by root ID.
    let mut roots: Vec<WsSummary> = Vec::new();
    let mut children_by_root: std::collections::HashMap<String, Vec<WsSummary>> =
        std::collections::HashMap::new();

    for item in items.iter() {
        if let Some(root_id) = &item.parent {
            children_by_root
                .entry(root_id.clone())
                .or_default()
                .push(item.clone());
        } else {
            roots.push(item.clone());
        }
    }

    // Sorting comparator for individual items (children or standalone roots).
    let item_cmp = |a: &WsSummary, b: &WsSummary| {
        b.attention
            .severity()
            .cmp(&a.attention.severity())
            .then(b.lifecycle.sidebar_rank().cmp(&a.lifecycle.sidebar_rank()))
            .then(b.created_at.cmp(&a.created_at))
            .then(a.name.cmp(&b.name))
    };

    let child_cmp =
        |a: &WsSummary, b: &WsSummary| a.finished.cmp(&b.finished).then_with(|| item_cmp(a, b));

    for children in children_by_root.values_mut() {
        children.sort_by(child_cmp);
    }

    // 3. Assemble and rank groups.
    struct Group {
        root: WsSummary,
        children: Vec<WsSummary>,
        worst_attention_severity: u8,
        best_lifecycle_rank: u8,
        group_needs_you: bool,
    }

    let mut groups: Vec<Group> = roots
        .into_iter()
        .map(|root| {
            let children = children_by_root.remove(&root.id).unwrap_or_default();
            let mut worst_severity = root.attention.severity();
            let mut best_rank = root.lifecycle.sidebar_rank();
            let mut any_needs_you = needs_you(root.attention);

            for c in &children {
                worst_severity = worst_severity.max(c.attention.severity());
                best_rank = best_rank.max(c.lifecycle.sidebar_rank());
                if needs_you(c.attention) {
                    any_needs_you = true;
                }
            }

            Group {
                root,
                children,
                worst_attention_severity: worst_severity,
                best_lifecycle_rank: best_rank,
                group_needs_you: any_needs_you,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        b.worst_attention_severity
            .cmp(&a.worst_attention_severity)
            .then(b.best_lifecycle_rank.cmp(&a.best_lifecycle_rank))
            .then(b.root.created_at.cmp(&a.root.created_at))
            .then(a.root.name.cmp(&b.root.name))
    });

    // 4. Flatten back into the slice with group_needs_you updated on each item.
    let mut result = Vec::with_capacity(items.len());
    for mut group in groups {
        group.root.group_needs_you = group.group_needs_you;
        result.push(group.root);
        for mut child in group.children {
            child.group_needs_you = group.group_needs_you;
            result.push(child);
        }
    }

    items.clone_from_slice(&result);
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

/// Tints a workspace mark can take; `mark_tint` picks one by id.
const MARK_TINTS: usize = 4;

/// Two-letter monogram: initials of the first two words ("api-server"
/// → "AS"), else the first two letters ("simone" → "SI").
pub fn monogram(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

/// Stable tint index keyed by workspace id, so renames keep the colour.
pub fn mark_tint(id: &str) -> usize {
    id.bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize))
        % MARK_TINTS
}

/// The workspace's mark: monogram on a stable tint. Shared by the
/// sidebar row and the content header so the two read as one place.
pub fn mark() -> gtk4::Label {
    let l = gtk4::Label::new(None);
    l.add_css_class("ws-mark");
    l.set_valign(gtk4::Align::Center);
    l.set_accessible_role(gtk4::AccessibleRole::Presentation);
    l
}

pub fn set_mark(l: &gtk4::Label, id: &str, name: &str) {
    l.set_text(&monogram(name));
    for i in 0..MARK_TINTS {
        l.remove_css_class(&format!("tint-{i}"));
    }
    l.add_css_class(&format!("tint-{}", mark_tint(id)));
}

/// Display name for real agents; shells and unknown commands have none.
pub fn agent_name(kind: AgentKind) -> Option<&'static str> {
    match kind {
        AgentKind::Claude => Some("Claude"),
        AgentKind::Codex => Some("Codex"),
        AgentKind::Opencode => Some("opencode"),
        AgentKind::Cursor => Some("Cursor"),
        AgentKind::Pi => Some("Pi"),
        AgentKind::Generic | AgentKind::None => None,
    }
}

/// Pure helper calculating the text for the disclosure button on a parent workspace row.
pub fn disclosure_label(
    active_count: usize,
    needs_count: usize,
    finished_count: usize,
    is_collapsed: bool,
) -> Option<String> {
    if active_count == 0 && finished_count == 0 {
        return None;
    }
    let arrow = if is_collapsed { "▸" } else { "▾" };
    let mut parts = Vec::new();

    if active_count > 0 {
        if active_count == 1 {
            parts.push("1 task".to_string());
        } else {
            parts.push(format!("{active_count} tasks"));
        }
        if needs_count > 0 {
            if needs_count == 1 {
                parts.push("1 need you".to_string());
            } else {
                parts.push(format!("{needs_count} need you"));
            }
        }
    }

    if finished_count > 0 {
        parts.push(format!("{finished_count} done"));
    }

    Some(format!("{arrow} {}", parts.join(" · ")))
}

type MenuBuilderCallback = Rc<RefCell<Option<Box<dyn Fn(&str) -> gio::Menu>>>>;

struct Row {
    id: String,
    parent: Option<String>,
    row: gtk4::ListBoxRow,
    mark: gtk4::Label,
    name: gtk4::Label,
    status: StatusSlot,
    message: gtk4::Label,
    activity: gtk4::Box,
    place: gtk4::Label,
    agents: gtk4::Label,
    metadata: gtk4::Box,
    disclosure: gtk4::Button,
    close: gtk4::Button,
    popover: gtk4::PopoverMenu,
    tasks: gtk4::Box,
    chips: Vec<TaskChipWidget>,
    chip_summary: String,
    /// Kept so the 30s tick can re-render time-derived text.
    summary: Option<WsSummary>,
    child_count: usize,
    child_needs_count: usize,
    child_finished_count: usize,
    is_collapsed: bool,
}

impl Drop for Row {
    fn drop(&mut self) {
        if self.popover.parent().is_some() {
            self.popover.unparent();
        }
    }
}

impl Row {
    fn new(
        id: &str,
        list: &gtk4::ListBox,
        on_close: &CloseCallback,
        on_toggle: &ToggleCallback,
        menu_builder: &MenuBuilderCallback,
    ) -> Row {
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
        let tasks = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        tasks.add_css_class("workspace-tasks");
        tasks.set_valign(gtk4::Align::Center);
        tasks.set_visible(false);
        let place = label(&["workspace-meta"]);
        place.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        place.set_hexpand(true);
        let agents = label(&["workspace-meta"]);
        agents.set_halign(gtk4::Align::End);
        let mark = mark();
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

        let disclosure = gtk4::Button::new();
        disclosure.add_css_class("flat");
        disclosure.add_css_class("workspace-disclosure");
        disclosure.set_valign(gtk4::Align::Center);
        disclosure.set_visible(false);
        let toggle_id = id.to_string();
        let on_toggle = Rc::clone(on_toggle);
        disclosure.connect_clicked(move |_| {
            if let Some(cb) = on_toggle.borrow().as_ref() {
                cb(toggle_id.clone());
            }
        });

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(8);
        grid.set_row_spacing(4);
        grid.add_css_class("workspace-row");
        let title = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        title.append(&mark);
        title.append(&name);
        grid.attach(&title, 0, 0, 1, 1);
        grid.attach(&status.widget, 1, 0, 1, 1);
        grid.attach(&close, 2, 0, 1, 1);
        let activity = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        activity.append(&tasks);
        activity.append(&message);
        grid.attach(&activity, 0, 1, 3, 1);
        let metadata = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        metadata.append(&disclosure);
        metadata.append(&place);
        metadata.append(&agents);
        grid.attach(&metadata, 0, 2, 3, 1);

        let row = gtk4::ListBoxRow::new();
        row.set_widget_name(id);
        row.set_child(Some(&grid));

        let popover = gtk4::PopoverMenu::from_model(None::<&gio::MenuModel>);
        popover.set_parent(&row);
        // Anchored at the pointer, like any context menu: no arrow.
        popover.set_has_arrow(false);

        // Secondary click (button 3) context menu
        let click_gesture = gtk4::GestureClick::new();
        click_gesture.set_button(3);
        let list_weak = list.downgrade();
        let popover_click = popover.clone();
        let row_click = row.clone();
        let id_click = id.to_string();
        let menu_builder_click = Rc::clone(menu_builder);
        click_gesture.connect_pressed(move |g, _n_press, x, y| {
            g.set_state(gtk4::EventSequenceState::Claimed);
            // Selecting activates the workspace (`on_select`), so every
            // `win.*` item acts on this row.
            if let Some(list) = list_weak.upgrade() {
                list.select_row(Some(&row_click));
            }
            if let Some(ref builder) = *menu_builder_click.borrow() {
                let menu = builder(&id_click);
                popover_click.set_menu_model(Some(&menu));
            }
            let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            popover_click.set_pointing_to(Some(&rect));
            popover_click.popup();
        });
        row.add_controller(click_gesture);

        // Keyboard context menu: Menu key or Shift+F10
        let key_controller = gtk4::EventControllerKey::new();
        let list_weak = list.downgrade();
        let popover_key = popover.clone();
        let row_key = row.clone();
        let id_key = id.to_string();
        let menu_builder_key = Rc::clone(menu_builder);
        key_controller.connect_key_pressed(move |_, keyval, _keycode, state| {
            let is_menu = keyval == gtk4::gdk::Key::Menu;
            let is_shift_f10 = keyval == gtk4::gdk::Key::F10
                && state.contains(gtk4::gdk::ModifierType::SHIFT_MASK);
            if is_menu || is_shift_f10 {
                if let Some(list) = list_weak.upgrade() {
                    list.select_row(Some(&row_key));
                }
                if let Some(ref builder) = *menu_builder_key.borrow() {
                    let menu = builder(&id_key);
                    popover_key.set_menu_model(Some(&menu));
                }
                popover_key.set_pointing_to(None);
                popover_key.popup();
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        row.add_controller(key_controller);

        Row {
            id: id.to_string(),
            parent: None,
            row,
            mark,
            name,
            status,
            message,
            activity,
            place,
            agents,
            metadata,
            disclosure,
            close,
            popover,
            tasks,
            chips: Vec::new(),
            chip_summary: String::new(),
            summary: None,
            child_count: 0,
            child_needs_count: 0,
            child_finished_count: 0,
            is_collapsed: false,
        }
    }

    /// Reconcile worker chips on this row. Hidden when the workspace
    /// has no task panes, so ordinary rows keep their three lines.
    fn set_chips(&mut self, views: &[TaskChipView]) {
        while self.chips.len() < views.len() {
            let chip = TaskChipWidget::new();
            self.tasks.append(&chip.root);
            self.chips.push(chip);
        }
        for (i, chip) in self.chips.iter().enumerate() {
            chip.set(views.get(i));
        }
        self.tasks.set_visible(!views.is_empty());
        self.chip_summary = views
            .iter()
            .map(|view| view.accessible_name.clone())
            .collect::<Vec<_>>()
            .join("; ");
        self.refresh_time();
    }

    fn update(&mut self, s: WsSummary) {
        self.parent = s.parent.clone();
        if s.group_needs_you {
            self.row.add_css_class(NEEDS_YOU);
        } else {
            self.row.remove_css_class(NEEDS_YOU);
        }
        if s.parent.is_some() && s.finished {
            self.row.add_css_class("workspace-finished");
        } else {
            self.row.remove_css_class("workspace-finished");
        }
        // Same-name workspaces are otherwise indistinguishable rows;
        // the unique handle tells them apart (spec 001 amendment).
        match &s.disambiguator {
            Some(handle) => self.name.set_text(&format!("{} · {handle}", s.name)),
            None => self.name.set_text(&s.name),
        }
        self.name.set_tooltip_text(Some(&self.name.text()));
        set_mark(&self.mark, &self.id, &s.name);
        self.place.set_text(&s.place);
        self.place.set_tooltip_text(Some(&s.place));
        self.agents.set_text(&s.agents);
        self.agents.set_tooltip_text(Some(&s.agents));
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

    fn set_child_mode(&mut self, is_child: bool) {
        if is_child {
            self.row.add_css_class("workspace-child");
            self.mark.set_visible(false);
            self.metadata.set_visible(false);
            if let Some(s) = &self.summary {
                let meta_tip = match (s.place.is_empty(), s.agents.is_empty()) {
                    (false, false) => format!("{} · {}", s.place, s.agents),
                    (false, true) => s.place.clone(),
                    (true, false) => s.agents.clone(),
                    (true, true) => String::new(),
                };
                self.row.set_tooltip_text(if meta_tip.is_empty() {
                    None
                } else {
                    Some(&meta_tip)
                });
            }
        } else {
            self.row.remove_css_class("workspace-child");
            self.row.remove_css_class("workspace-finished");
            self.mark.set_visible(true);
            self.metadata.set_visible(true);
            self.row.set_tooltip_text(None);
        }
    }

    fn update_disclosure(
        &mut self,
        count: usize,
        needs_count: usize,
        finished_count: usize,
        is_collapsed: bool,
    ) {
        self.child_count = count;
        self.child_needs_count = needs_count;
        self.child_finished_count = finished_count;
        self.is_collapsed = is_collapsed;
        self.render_disclosure();
    }

    fn set_collapsed(&mut self, is_collapsed: bool) {
        self.is_collapsed = is_collapsed;
        self.render_disclosure();
    }

    fn render_disclosure(&self) {
        if let Some(label) = disclosure_label(
            self.child_count,
            self.child_needs_count,
            self.child_finished_count,
            self.is_collapsed,
        ) {
            self.disclosure.set_visible(true);
            self.disclosure.set_label(&label);
            let action_str = if self.is_collapsed {
                "Expand"
            } else {
                "Collapse"
            };
            // "Collapse 3 tasks · 2 done": the visible text minus the arrow.
            let counts = label.split_once(' ').map_or("", |(_, rest)| rest);
            let acc_label = format!("{action_str} {counts}");
            self.disclosure
                .update_property(&[gtk4::accessible::Property::Label(&acc_label)]);
        } else {
            self.disclosure.set_visible(false);
        }
    }

    fn refresh_time(&self) {
        let Some(s) = &self.summary else { return };
        let headline = s.headline(Utc::now());
        let description = [
            self.name.text().to_string(),
            self.chip_summary.clone(),
            status::attention_tooltip(s.attention).to_string(),
            headline.clone(),
            s.place.clone(),
            s.agents.clone(),
        ]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        self.row
            .update_property(&[gtk4::accessible::Property::Label(&description)]);
        if self.message.text() != headline {
            self.message.set_text(&headline);
            self.message.set_tooltip_text(Some(&headline));
        }
        // A plain shell's headline is the bare "Running": nothing a
        // second line should spend height on, so the row drops to name
        // and place (spec 039).
        self.activity
            .set_visible(!quiet_headline(s) || self.tasks.get_visible());
        let time = s.last_activity.map(time_ago).unwrap_or_default();
        // Attention dates from when it was raised; lifecycle from its change.
        let began = if s.attention == Attention::None {
            s.lifecycle_since
        } else {
            s.attention_since
        };
        let age = began.map(time_ago).unwrap_or_default();
        self.status.set(s.lifecycle, s.attention, &time, &age);
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
type ToggleCallback = Rc<RefCell<Option<Box<dyn Fn(String)>>>>;

pub struct Sidebar {
    pub widget: gtk4::ScrolledWindow,
    count: gtk4::Label,
    list: gtk4::ListBox,
    rows: Rc<RefCell<Vec<Row>>>,
    collapsed_roots: Rc<RefCell<std::collections::HashSet<String>>>,
    on_select: SelectCallback,
    on_close: CloseCallback,
    on_toggle: ToggleCallback,
    menu_builder: MenuBuilderCallback,
}

// These are navigation shortcuts to existing window actions, not a
// parallel action registry. Their handlers and keybindings stay in actions.rs.
fn tool_button(icon: &str, title: &str, hint: &str, action: &str) -> gtk4::Button {
    let button = gtk4::Button::new();
    button.add_css_class("sidebar-tool-button");
    button.set_action_name(Some(action));
    button.set_tooltip_text(Some(hint));
    button.update_property(&[gtk4::accessible::Property::Label(title)]);
    let contents = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let image = gtk4::Image::from_icon_name(icon);
    image.add_css_class("sidebar-tool-icon");
    contents.append(&image);
    let label = gtk4::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    contents.append(&label);
    button.set_child(Some(&contents));
    button
}

impl Sidebar {
    pub fn new() -> Sidebar {
        let list = gtk4::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_selection_mode(gtk4::SelectionMode::Single);
        list.set_header_func(apply_section_header);
        let on_select: SelectCallback = Rc::new(RefCell::new(None));
        let on_close: CloseCallback = Rc::new(RefCell::new(None));
        let on_toggle: ToggleCallback = Rc::new(RefCell::new(None));
        let menu_builder: MenuBuilderCallback = Rc::new(RefCell::new(None));
        let rows: Rc<RefCell<Vec<Row>>> = Rc::new(RefCell::new(Vec::new()));
        let collapsed_roots: Rc<RefCell<std::collections::HashSet<String>>> =
            Rc::new(RefCell::new(std::collections::HashSet::new()));
        {
            let on_select = Rc::clone(&on_select);
            list.connect_row_selected(move |_, row| {
                if let (Some(row), Some(cb)) = (row, on_select.borrow().as_ref()) {
                    cb(row.widget_name().to_string());
                }
            });
        }
        {
            let rows = Rc::clone(&rows);
            let collapsed_roots = Rc::clone(&collapsed_roots);
            let list_weak = list.downgrade();
            *on_toggle.borrow_mut() = Some(Box::new(move |root_id: String| {
                let is_collapsed = {
                    let mut collapsed = collapsed_roots.borrow_mut();
                    if collapsed.contains(&root_id) {
                        collapsed.remove(&root_id);
                        false
                    } else {
                        collapsed.insert(root_id.clone());
                        true
                    }
                };
                for r in rows.borrow_mut().iter_mut() {
                    if r.parent.as_deref() == Some(&root_id) {
                        r.row.set_visible(!is_collapsed);
                    }
                    if r.id == root_id {
                        r.set_collapsed(is_collapsed);
                    }
                }
                if let Some(list) = list_weak.upgrade() {
                    list.invalidate_headers();
                }
            }));
        }
        // Scroll the project list and tool shortcuts as one surface. The
        // pinned Search launcher and Preferences footer remain reachable.
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let workspaces_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        workspaces_header.add_css_class("sidebar-group-header");
        let workspaces_title = gtk4::Label::new(Some("WORKSPACES"));
        workspaces_title.add_css_class("sidebar-group-label");
        workspaces_title.set_xalign(0.0);
        workspaces_title.set_hexpand(true);
        let count = gtk4::Label::new(Some("0"));
        count.add_css_class("sidebar-group-count");
        count.add_css_class("numeric");
        count.update_property(&[gtk4::accessible::Property::Label("0 workspaces")]);
        workspaces_header.append(&workspaces_title);
        workspaces_header.append(&count);
        content.append(&workspaces_header);
        content.append(&list);

        let tools_header = gtk4::Label::new(Some("TOOLS"));
        tools_header.add_css_class("sidebar-group-label");
        tools_header.add_css_class("sidebar-tools-header");
        tools_header.set_xalign(0.0);
        content.append(&tools_header);
        let tools = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        tools.add_css_class("sidebar-tools");
        for (icon, title, hint, action) in [
            (
                "signaltty-board-symbolic",
                "Task Board",
                "Open Task Board (Ctrl+Shift+B)",
                "win.show-board",
            ),
            (
                "sidebar-show-right-symbolic",
                "Changes",
                "Toggle Changes (Ctrl+Shift+D)",
                "win.show-changes",
            ),
            (
                "signaltty-branch-symbolic",
                "Worktrees",
                "Manage Git Worktrees",
                "win.worktrees",
            ),
        ] {
            tools.append(&tool_button(icon, title, hint, action));
        }
        content.append(&tools);

        let widget = gtk4::ScrolledWindow::new();
        widget.set_child(Some(&content));
        widget.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        widget.set_vexpand(true);
        Sidebar {
            widget,
            count,
            list,
            rows,
            collapsed_roots,
            on_select,
            on_close,
            on_toggle,
            menu_builder,
        }
    }

    pub fn set_menu_builder<F: Fn(&str) -> gio::Menu + 'static>(&self, cb: F) {
        *self.menu_builder.borrow_mut() = Some(Box::new(cb));
    }

    pub fn set_on_select(&self, cb: impl Fn(String) + 'static) {
        *self.on_select.borrow_mut() = Some(Box::new(cb));
    }

    pub fn set_on_close(&self, cb: impl Fn(String) + 'static) {
        *self.on_close.borrow_mut() = Some(Box::new(cb));
    }

    /// Reconcile rows with `items` (priority order), updating in
    /// place. Moves remove + re-insert rows, which drops GTK's selection
    /// on the moved row — so snapshot the selected workspace and restore
    /// it after: the highlight tracks the workspace, not the row index.
    /// The echo is harmless (`on_select` skips the already-active one).
    pub fn update(&self, items: Vec<WsSummary>) {
        self.count.set_text(&items.len().to_string());
        let noun = if items.len() == 1 {
            "workspace"
        } else {
            "workspaces"
        };
        self.count
            .update_property(&[gtk4::accessible::Property::Label(&format!(
                "{} {noun}",
                items.len()
            ))]);
        let selected = self
            .list
            .selected_row()
            .map(|r| r.widget_name().to_string());
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
                Some(pos) if pos == i => rows[i].update(item.clone()),
                Some(pos) => {
                    let mut row = rows.remove(pos);
                    self.list.remove(&row.row);
                    self.list.insert(&row.row, i as i32);
                    row.update(item.clone());
                    rows.insert(i, row);
                }
                None => {
                    let mut row = Row::new(
                        &item.id,
                        &self.list,
                        &self.on_close,
                        &self.on_toggle,
                        &self.menu_builder,
                    );
                    row.update(item.clone());
                    self.list.insert(&row.row, i as i32);
                    rows.insert(i, row);
                }
            }
        }

        let mut child_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut child_needs_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut child_finished_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for item in &items {
            if let Some(parent_id) = &item.parent {
                if item.finished {
                    *child_finished_counts.entry(parent_id.clone()).or_default() += 1;
                } else {
                    *child_counts.entry(parent_id.clone()).or_default() += 1;
                    if needs_you(item.attention) {
                        *child_needs_counts.entry(parent_id.clone()).or_default() += 1;
                    }
                }
            }
        }

        self.collapsed_roots
            .borrow_mut()
            .retain(|id| child_counts.contains_key(id) || child_finished_counts.contains_key(id));

        let collapsed = self.collapsed_roots.borrow();
        for row in rows.iter_mut() {
            let is_child = row.parent.is_some();
            row.set_child_mode(is_child);
            if let Some(parent_id) = &row.parent {
                let is_hidden = collapsed.contains(parent_id);
                row.row.set_visible(!is_hidden);
            } else {
                row.row.set_visible(true);
                let count = child_counts.get(&row.id).copied().unwrap_or(0);
                let needs = child_needs_counts.get(&row.id).copied().unwrap_or(0);
                let finished = child_finished_counts.get(&row.id).copied().unwrap_or(0);
                let is_collapsed = collapsed.contains(&row.id);
                row.update_disclosure(count, needs, finished, is_collapsed);
            }
        }
        drop(collapsed);
        drop(rows);

        if let Some(id) = selected {
            if self.rows.borrow().iter().any(|r| r.id == id) {
                self.select(&id);
            }
        }
        // Section membership can change without a move.
        self.list.invalidate_headers();
    }

    /// Worker chips for workspaces that own task panes. Rows with no
    /// entry hide the chip box. Updates the existing chips in place.
    pub fn set_task_chips(
        &self,
        by_workspace: &std::collections::HashMap<String, Vec<TaskChipView>>,
    ) {
        let empty = Vec::new();
        for row in self.rows.borrow_mut().iter_mut() {
            row.set_chips(by_workspace.get(&row.id).unwrap_or(&empty));
        }
    }

    /// Re-render relative times ("now" → "2m") between server events.
    pub fn refresh_times(&self) {
        for r in self.rows.borrow().iter() {
            r.refresh_time();
        }
    }

    pub fn select(&self, ws_id: &str) {
        let mut rows = self.rows.borrow_mut();
        if let Some(pos) = rows.iter().position(|r| r.id == ws_id) {
            if let Some(parent_id) = rows[pos].parent.clone() {
                if self.collapsed_roots.borrow().contains(&parent_id) {
                    self.collapsed_roots.borrow_mut().remove(&parent_id);
                    for r in rows.iter_mut() {
                        if r.parent.as_deref() == Some(&parent_id) {
                            r.row.set_visible(true);
                        }
                        if r.id == parent_id {
                            r.set_collapsed(false);
                        }
                    }
                    self.list.invalidate_headers();
                }
            }
            if !rows[pos].row.is_selected() {
                self.list.select_row(Some(&rows[pos].row));
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
            disambiguator: None,
            parent: None,
            finished: false,
            group_needs_you: false,
            created_at: Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap(),
            lifecycle,
            attention,
            message: None,
            lifecycle_since: None,
            attention_since: None,
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
    fn creation_then_name_break_ties_deterministically() {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let at = |name: &str, created_mins_ago: i64| WsSummary {
            created_at: t0 - chrono::Duration::minutes(created_mins_ago),
            ..summary(name, Lifecycle::Idle, Attention::None, Some(1))
        };
        // Newer workspaces first; names break created ties.
        let mut items = vec![at("b-old", 30), at("a-new", 1), at("c-new", 1)];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["a-new", "c-new", "b-old"]);
        let mut again = vec![at("b-old", 30), at("c-new", 1), at("a-new", 1)];
        sort_summaries(&mut again);
        assert_eq!(names(&again), names(&items), "order input-independent");
    }

    #[test]
    fn order_survives_activity_churn() {
        // Regression test for rows flapping under the pointer: agent events
        // rewrite last_activity constantly, and the order must not follow —
        // severity and rank float urgent work instead.
        let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let at = |name: &str, created_mins_ago: i64, active_mins_ago: i64| WsSummary {
            created_at: t0 - chrono::Duration::minutes(created_mins_ago),
            ..summary(
                name,
                Lifecycle::Working,
                Attention::None,
                Some(active_mins_ago),
            )
        };
        let mut items = vec![at("a", 60, 50), at("b", 30, 1)];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["b", "a"]);
        // Fresh tool events on `a` (activity now) must not move it.
        let mut churned = vec![at("a", 60, 0), at("b", 30, 1)];
        sort_summaries(&mut churned);
        assert_eq!(
            names(&churned),
            ["b", "a"],
            "activity churn must not reorder"
        );
    }

    #[test]
    fn children_follow_their_root() {
        let mut r1 = summary("root1", Lifecycle::Idle, Attention::None, Some(0));
        r1.created_at = Utc.with_ymd_and_hms(2026, 9, 28, 10, 0, 0).unwrap();
        let mut c1_1 = summary("child1-1", Lifecycle::Working, Attention::None, Some(0));
        c1_1.parent = Some("root1".into());
        let mut c1_2 = summary("child1-2", Lifecycle::Done, Attention::None, Some(0));
        c1_2.parent = Some("root1".into());

        let mut r2 = summary("root2", Lifecycle::Idle, Attention::None, Some(0));
        r2.created_at = Utc.with_ymd_and_hms(2026, 9, 28, 11, 0, 0).unwrap();

        // Without grouping, r2 would sort before r1 (newer created_at).
        // With grouping, root1's best lifecycle rank is Done (rank 3), beating root2's Idle (rank 1).
        let mut items = vec![r2, r1, c1_2, c1_1];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["root1", "child1-2", "child1-1", "root2"]);
    }

    #[test]
    fn root_order_uses_group_severity_floating_calm_root() {
        let calm_root = summary("calm-root", Lifecycle::Idle, Attention::None, Some(0));
        let mut needy_child = summary(
            "needy-child",
            Lifecycle::Working,
            Attention::PermissionRequired,
            Some(0),
        );
        needy_child.parent = Some("calm-root".into());

        let other_root = summary("other-root", Lifecycle::Blocked, Attention::None, Some(0));

        let mut items = vec![other_root, calm_root, needy_child];
        sort_summaries(&mut items);
        assert_eq!(names(&items), ["calm-root", "needy-child", "other-root"]);
        assert!(items[0].group_needs_you);
        assert!(items[1].group_needs_you);
        assert!(!items[2].group_needs_you);
    }

    #[test]
    fn orphan_child_becomes_root() {
        let mut orphan = summary("orphan", Lifecycle::Idle, Attention::None, Some(0));
        orphan.parent = Some("missing-parent".into());
        let normal = summary("normal", Lifecycle::Idle, Attention::None, Some(0));

        let mut items = vec![orphan, normal];
        sort_summaries(&mut items);
        assert_eq!(items[0].parent, None);
        assert_eq!(items[1].parent, None);
    }

    #[test]
    fn nested_parent_chain_flattens_to_root() {
        let root = summary("root", Lifecycle::Idle, Attention::None, Some(0));
        let mut child = summary("child", Lifecycle::Idle, Attention::None, Some(0));
        child.parent = Some("root".into());
        let mut grandchild = summary("grandchild", Lifecycle::Idle, Attention::None, Some(0));
        grandchild.parent = Some("child".into());

        let mut items = vec![grandchild, child, root];
        sort_summaries(&mut items);
        assert_eq!(items[0].name, "root");
        assert_eq!(items[0].parent, None);
        assert_eq!(items[1].parent, Some("root".into()));
        assert_eq!(items[2].parent, Some("root".into()));
    }

    #[test]
    fn cycle_terminates() {
        let mut a = summary("a", Lifecycle::Idle, Attention::None, Some(0));
        a.parent = Some("b".into());
        let mut b = summary("b", Lifecycle::Idle, Attention::None, Some(0));
        b.parent = Some("a".into());

        let mut items = vec![a, b];
        sort_summaries(&mut items);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].parent, None);
        assert_eq!(items[1].parent, None);
    }

    #[test]
    fn monogram_takes_word_initials_or_two_letters() {
        assert_eq!(monogram("api-server"), "AS");
        assert_eq!(monogram("signaltty gui app"), "SG");
        assert_eq!(monogram("simone"), "SI");
        assert_eq!(monogram("x"), "X");
        assert_eq!(monogram("--"), "?");
        assert_eq!(monogram("èco"), "ÈC");
    }

    #[test]
    fn mark_tint_is_stable_and_in_range() {
        assert_eq!(mark_tint("ws_a"), mark_tint("ws_a"));
        let tints: std::collections::HashSet<_> =
            (0..32).map(|i| mark_tint(&format!("ws_{i}"))).collect();
        assert!(tints.iter().all(|t| *t < MARK_TINTS));
        assert_eq!(tints.len(), MARK_TINTS, "ids spread over every tint");
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

    #[test]
    #[ignore = "requires a GTK display; run with dbus-run-session"]
    fn task_chips_bring_back_a_quiet_rows_activity_line() {
        gtk4::init().unwrap();
        let sidebar = Sidebar::new();
        sidebar.update(vec![summary(
            "ws",
            Lifecycle::Unknown,
            Attention::None,
            None,
        )]);
        let activity = |sidebar: &Sidebar| sidebar.rows.borrow()[0].activity.get_visible();
        assert!(
            !activity(&sidebar),
            "plain shell row drops its headline line"
        );
        let chip = TaskChipView {
            kind: "Task",
            label: "fix".into(),
            show_label: true,
            lineage: None,
            state: "Working",
            css_class: "task-working",
            tooltip: "fix".into(),
            accessible_name: "Task fix, Working".into(),
        };
        sidebar.set_task_chips(&[("ws".to_string(), vec![chip])].into());
        assert!(activity(&sidebar), "chips live on that line, so it returns");
        sidebar.set_task_chips(&Default::default());
        assert!(!activity(&sidebar));
    }

    #[test]
    fn only_a_bare_running_headline_is_quiet() {
        let mut s = summary("ws", Lifecycle::Unknown, Attention::None, None);
        assert!(quiet_headline(&s), "plain shell says only Running");
        s.message = Some("Built the docs".into());
        assert!(!quiet_headline(&s), "an explicit message always shows");
        let s = summary("ws", Lifecycle::Idle, Attention::None, None);
        assert!(!quiet_headline(&s), "agent states keep their line");
    }

    fn workspace() -> Workspace {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        Workspace {
            id: "ws".into(),
            name: "ws".into(),
            handle: "ws".into(),
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
    fn attention_age_comes_from_the_gate_not_later_activity() {
        let t0 = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let mut gate = pane(Lifecycle::Blocked, None);
        gate.attention = Attention::PermissionRequired;
        gate.attention_since = Some(t0 - chrono::Duration::minutes(10));
        let mut chatty = pane(Lifecycle::Working, None);
        chatty.last_activity_at = t0;
        let s = summarize(&workspace(), &[gate, chatty]);
        assert_eq!(s.attention, Attention::PermissionRequired);
        assert_eq!(s.attention_since, Some(t0 - chrono::Duration::minutes(10)));
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

    #[test]
    fn restored_working_pane_cannot_supply_a_live_siblings_elapsed_time() {
        let now = Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap();
        let mut restored = pane(
            Lifecycle::Working,
            Some(now - chrono::Duration::minutes(45)),
        );
        restored.live = signaltty_core::LiveState::Exited { code: None };
        restored.restore_state = signaltty_core::RestoreState::Restored;
        let running = pane(Lifecycle::Working, Some(now - chrono::Duration::minutes(2)));
        let summary = summarize(&workspace(), &[restored.clone(), running]);
        assert_eq!(summary.lifecycle, Lifecycle::Working);
        assert_eq!(summary.headline(now), "Working for 2m…");

        let stopped = summarize(&workspace(), &[restored]);
        assert_eq!(stopped.lifecycle, Lifecycle::Exited);
        assert_eq!(stopped.headline(now), "Exited");
    }

    #[test]
    fn sort_summaries_finished_children_sort_after_unfinished_siblings() {
        let root = summary("root", Lifecycle::Idle, Attention::None, Some(0));

        let mut child_unfinished_working = summary(
            "child-unfin-work",
            Lifecycle::Working,
            Attention::None,
            Some(0),
        );
        child_unfinished_working.parent = Some("root".into());
        child_unfinished_working.finished = false;

        let mut child_finished_working = summary(
            "child-fin-work",
            Lifecycle::Working,
            Attention::None,
            Some(0),
        );
        child_finished_working.parent = Some("root".into());
        child_finished_working.finished = true;

        let mut child_unfinished_idle = summary(
            "child-unfin-idle",
            Lifecycle::Idle,
            Attention::None,
            Some(0),
        );
        child_unfinished_idle.parent = Some("root".into());
        child_unfinished_idle.finished = false;

        let mut child_finished_idle =
            summary("child-fin-idle", Lifecycle::Idle, Attention::None, Some(0));
        child_finished_idle.parent = Some("root".into());
        child_finished_idle.finished = true;

        let mut items = vec![
            child_finished_working,
            child_unfinished_idle,
            root,
            child_finished_idle,
            child_unfinished_working,
        ];
        sort_summaries(&mut items);

        // Group ordering: root is at 0, followed by children
        assert_eq!(items[0].name, "root");
        // Unfinished children sort before finished children:
        // Among unfinished: child-unfin-work (Working) ranks before child-unfin-idle (Idle)
        assert_eq!(items[1].name, "child-unfin-work");
        assert_eq!(items[2].name, "child-unfin-idle");
        // Among finished: child-fin-work (Working) ranks before child-fin-idle (Idle)
        assert_eq!(items[3].name, "child-fin-work");
        assert_eq!(items[4].name, "child-fin-idle");
    }

    #[test]
    fn disclosure_label_formatting() {
        // No tasks -> None
        assert_eq!(disclosure_label(0, 0, 0, false), None);
        assert_eq!(disclosure_label(0, 0, 0, true), None);

        // Only active
        assert_eq!(
            disclosure_label(1, 0, 0, false),
            Some("▾ 1 task".to_string())
        );
        assert_eq!(
            disclosure_label(3, 0, 0, true),
            Some("▸ 3 tasks".to_string())
        );

        // Active with needs you
        assert_eq!(
            disclosure_label(3, 1, 0, false),
            Some("▾ 3 tasks · 1 need you".to_string())
        );
        assert_eq!(
            disclosure_label(3, 2, 0, true),
            Some("▸ 3 tasks · 2 need you".to_string())
        );

        // Active with finished
        assert_eq!(
            disclosure_label(3, 0, 2, false),
            Some("▾ 3 tasks · 2 done".to_string())
        );

        // Active with needs you and finished
        assert_eq!(
            disclosure_label(3, 1, 2, false),
            Some("▾ 3 tasks · 1 need you · 2 done".to_string())
        );

        // Only finished (0 active)
        assert_eq!(
            disclosure_label(0, 0, 2, false),
            Some("▾ 2 done".to_string())
        );
        assert_eq!(
            disclosure_label(0, 0, 1, true),
            Some("▸ 1 done".to_string())
        );
    }
}
