//! Task board: multi-column view of orchestrated tasks grouped by what they need.
//!
//! Tasks are derived into five columns:
//! - Working: active tasks still running or pending.
//! - Needs you: tasks requiring input, failed, or rejected.
//! - In review: completed tasks waiting for diff review and merge/discard.
//! - Ready to merge: tasks with passing checks and review ready to merge.
//! - Done: archived tasks that were merged, discarded, or canceled.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Utc};
use gtk4::prelude::*;
use libadwaita::prelude::*;
use signaltty_core::{DispositionOutcome, PrChecks, PrReview, PrState, Task, TaskState};

use crate::task_chip::{state_class, state_word};

/// The five kanban columns representing a task's lifecycle stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoardColumn {
    Working,
    NeedsYou,
    InReview,
    ReadyToMerge,
    Done,
}

impl BoardColumn {
    pub fn title(self) -> &'static str {
        match self {
            BoardColumn::Working => "Working",
            BoardColumn::NeedsYou => "Needs you",
            BoardColumn::InReview => "In review",
            BoardColumn::ReadyToMerge => "Ready to merge",
            BoardColumn::Done => "Done",
        }
    }
}

/// Pure derivation of a task's board column according to orchestrator lifecycle rules:
/// - Working: TaskState::Pending | TaskState::Working
/// - NeedsYou: InputRequired | Failed | Rejected (with outcome == None)
/// - InReview: Completed (with outcome == None)
/// - Done: disposition outcome Merged or Discarded, plus Canceled
pub fn board_column(task: &Task) -> BoardColumn {
    if matches!(
        task.disposition.outcome,
        DispositionOutcome::Merged | DispositionOutcome::Discarded
    ) || task.state == TaskState::Canceled
    {
        BoardColumn::Done
    } else if let Some(ref pr) = task.pr {
        match pr.state {
            PrState::Merged | PrState::Closed => BoardColumn::Done,
            PrState::Open => {
                if pr.checks == PrChecks::Failing || pr.review == PrReview::ChangesRequested {
                    BoardColumn::NeedsYou
                } else if pr.checked_at.is_some()
                    && matches!(pr.checks, PrChecks::Passing | PrChecks::None)
                    && matches!(pr.review, PrReview::Approved | PrReview::None)
                {
                    BoardColumn::ReadyToMerge
                } else {
                    BoardColumn::InReview
                }
            }
        }
    } else if task.state == TaskState::Completed
        && task.disposition.outcome == DispositionOutcome::None
    {
        BoardColumn::InReview
    } else if matches!(
        task.state,
        TaskState::InputRequired | TaskState::Failed | TaskState::Rejected
    ) && task.disposition.outcome == DispositionOutcome::None
    {
        BoardColumn::NeedsYou
    } else {
        BoardColumn::Working
    }
}

/// "now", "3m", "2h", "1d" relative age format from `updated_at` against `now`.
pub fn relative_age(updated_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - updated_at).num_seconds().max(0);
    if secs < 60 {
        "now".to_string()
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Fallback to short task id tail if label is empty.
fn display_label(task: &Task) -> String {
    let label = task.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    let tail = task
        .id
        .rsplit(['_', '-'])
        .next()
        .unwrap_or(task.id.as_str());
    let tail: String = tail.chars().take(8).collect();
    if tail.is_empty() {
        "task".to_string()
    } else {
        tail
    }
}

/// Pure presentation view struct for a single task card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskCardView {
    pub id: String,
    pub pane_id: Option<String>,
    pub label: String,
    pub agent: String,
    pub branch: String,
    pub state_word: &'static str,
    pub css_class: &'static str,
    pub relative_age: String,
    pub updated_at: DateTime<Utc>,
    pub pr_number: Option<u64>,
    pub pr_checks: Option<PrChecks>,
}

impl TaskCardView {
    /// Second line formatted as `agent · branch · age` plus optional PR info (filtering empty parts).
    pub fn subtitle(&self) -> String {
        let pr_num_str = self.pr_number.map(|n| format!("#{n}"));
        let checks_str = match self.pr_checks {
            Some(PrChecks::Failing) => Some("checks failing"),
            Some(PrChecks::Passing) => Some("checks passing"),
            Some(PrChecks::Pending) => Some("checks pending"),
            Some(PrChecks::None) | None => None,
        };

        let mut parts: Vec<&str> = [
            self.agent.as_str(),
            self.branch.as_str(),
            self.relative_age.as_str(),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();

        if let Some(ref num) = pr_num_str {
            parts.push(num.as_str());
        }
        if let Some(checks) = checks_str {
            parts.push(checks);
        }

        parts.join(" · ")
    }
}

/// Convert a domain `Task` to a pure presentation `TaskCardView`.
pub fn task_to_card(task: &Task, now: DateTime<Utc>) -> TaskCardView {
    let (pr_number, pr_checks) = match task.pr {
        Some(ref pr) => (Some(pr.number), Some(pr.checks)),
        None => (None, None),
    };
    TaskCardView {
        id: task.id.clone(),
        pane_id: task.pane_id.clone(),
        label: display_label(task),
        agent: task.agent.clone().unwrap_or_default(),
        branch: task.branch.clone(),
        state_word: state_word(task.state),
        css_class: state_class(task.state),
        relative_age: relative_age(task.updated_at, now),
        updated_at: task.updated_at,
        pr_number,
        pr_checks,
    }
}

/// Representation of a board column and its cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardColumnView {
    pub column: BoardColumn,
    pub total_count: usize,
    pub cards: Vec<TaskCardView>,
}

/// Build the 5 columns for a slice of tasks, sorting cards by `updated_at` descending.
/// The Done column shows at most 20 latest cards while retaining the total count.
pub fn build_board(tasks: &[Task], now: DateTime<Utc>) -> Vec<BoardColumnView> {
    let mut working = Vec::new();
    let mut needs_you = Vec::new();
    let mut in_review = Vec::new();
    let mut ready_to_merge = Vec::new();
    let mut done = Vec::new();

    for task in tasks {
        let col = board_column(task);
        let card = task_to_card(task, now);
        match col {
            BoardColumn::Working => working.push(card),
            BoardColumn::NeedsYou => needs_you.push(card),
            BoardColumn::InReview => in_review.push(card),
            BoardColumn::ReadyToMerge => ready_to_merge.push(card),
            BoardColumn::Done => done.push(card),
        }
    }

    let sort_desc = |cards: &mut Vec<TaskCardView>| {
        cards.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    };

    sort_desc(&mut working);
    sort_desc(&mut needs_you);
    sort_desc(&mut in_review);
    sort_desc(&mut ready_to_merge);
    sort_desc(&mut done);

    let done_total = done.len();
    if done.len() > 20 {
        done.truncate(20);
    }

    vec![
        BoardColumnView {
            column: BoardColumn::Working,
            total_count: working.len(),
            cards: working,
        },
        BoardColumnView {
            column: BoardColumn::NeedsYou,
            total_count: needs_you.len(),
            cards: needs_you,
        },
        BoardColumnView {
            column: BoardColumn::InReview,
            total_count: in_review.len(),
            cards: in_review,
        },
        BoardColumnView {
            column: BoardColumn::ReadyToMerge,
            total_count: ready_to_merge.len(),
            cards: ready_to_merge,
        },
        BoardColumnView {
            column: BoardColumn::Done,
            total_count: done_total,
            cards: done,
        },
    ]
}

struct CardWidgets {
    row: gtk4::ListBoxRow,
    title: gtk4::Label,
    pill: gtk4::Label,
    meta: gtk4::Label,
}

impl CardWidgets {
    fn new(card: &TaskCardView) -> Self {
        let row = gtk4::ListBoxRow::new();
        row.add_css_class("board-card");
        row.set_widget_name(&card.id);
        row.set_selectable(false);
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let title = gtk4::Label::new(None);
        title.add_css_class("board-card-title");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_hexpand(true);
        title.set_halign(gtk4::Align::Start);
        let pill = gtk4::Label::new(None);
        pill.add_css_class("task-chip");
        pill.add_css_class("board-card-pill");
        pill.set_halign(gtk4::Align::End);
        top.append(&title);
        top.append(&pill);
        body.append(&top);
        let meta = gtk4::Label::new(None);
        meta.add_css_class("board-card-meta");
        meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        meta.set_halign(gtk4::Align::Start);
        body.append(&meta);
        row.set_child(Some(&body));
        let widgets = Self {
            row,
            title,
            pill,
            meta,
        };
        widgets.update(card);
        widgets
    }

    fn update(&self, card: &TaskCardView) {
        self.row.set_activatable(card.pane_id.is_some());
        self.title.set_text(&card.label);
        self.pill.set_text(card.state_word);
        for class in crate::task_chip::TASK_CHIP_CLASSES {
            self.pill.remove_css_class(class);
        }
        self.pill.add_css_class(card.css_class);
        let subtitle = card.subtitle();
        self.meta.set_text(&subtitle);
        self.row.set_tooltip_text(Some(&format!(
            "{}: {}\n{}",
            card.label, card.state_word, subtitle
        )));
    }
}

struct ColumnWidgets {
    header: gtk4::Label,
    notice: gtk4::Label,
    scroll: gtk4::ScrolledWindow,
    list: gtk4::ListBox,
}

#[derive(Clone)]
struct RestorePoint {
    focus_epoch: u64,
    focused: Option<String>,
    neighbors: Option<(gtk4::ListBox, Option<String>, Option<String>)>,
    positions: Vec<(f64, Option<(String, f64)>)>,
    x: f64,
}

fn focus_target(
    rows: &std::collections::HashMap<String, CardWidgets>,
    point: &RestorePoint,
) -> Option<gtk4::ListBoxRow> {
    point
        .focused
        .as_ref()
        .and_then(|id| rows.get(id))
        .or_else(|| {
            let (list, next, previous) = point.neighbors.as_ref()?;
            [next, previous]
                .into_iter()
                .filter_map(|id| id.as_ref().and_then(|id| rows.get(id)))
                .find(|widgets| widgets.row.parent().as_ref() == Some(list.upcast_ref()))
        })
        .map(|widgets| widgets.row.clone())
}

/// A live board keeps its dialog and keyed task rows through server updates.
pub struct Board {
    pub dialog: libadwaita::Dialog,
    stack: gtk4::Stack,
    scroll: gtk4::ScrolledWindow,
    content: gtk4::Box,
    columns: Vec<ColumnWidgets>,
    rows: Rc<RefCell<std::collections::HashMap<String, CardWidgets>>>,
    pending: Rc<RefCell<Option<RestorePoint>>>,
    cards: Rc<RefCell<std::collections::HashMap<String, TaskCardView>>>,
    revision: Rc<std::cell::Cell<u64>>,
    focus_epoch: Rc<std::cell::Cell<u64>>,
    reconciling: Rc<std::cell::Cell<bool>>,
    closing: Rc<std::cell::Cell<bool>>,
}

impl Board {
    pub fn update(&self, tasks: &[Task]) {
        if self.closing.get() {
            return;
        }
        let focus = self.dialog.focus();
        let focused = self.rows.borrow().iter().find_map(|(id, widgets)| {
            focus
                .as_ref()
                .filter(|f| {
                    *f == widgets.row.upcast_ref::<gtk4::Widget>() || f.is_ancestor(&widgets.row)
                })
                .map(|_| id.clone())
        });
        let focused_neighbors = focused.as_ref().and_then(|id| {
            let rows = self.rows.borrow();
            let row = &rows.get(id)?.row;
            let list = row.parent()?.downcast::<gtk4::ListBox>().ok()?;
            Some((
                list.clone(),
                list.row_at_index(row.index() + 1)
                    .map(|r| r.widget_name().to_string()),
                list.row_at_index(row.index() - 1)
                    .map(|r| r.widget_name().to_string()),
            ))
        });
        let positions = self
            .columns
            .iter()
            .map(|col| {
                let y = col.scroll.vadjustment().value();
                let mut anchor = None;
                let mut index = 0;
                while let Some(row) = col.list.row_at_index(index) {
                    if let Some(bounds) = row.compute_bounds(&col.list) {
                        if f64::from(bounds.y() + bounds.height()) > y {
                            anchor =
                                Some((row.widget_name().to_string(), f64::from(bounds.y()) - y));
                            break;
                        }
                    }
                    index += 1;
                }
                (y, anchor)
            })
            .collect::<Vec<_>>();
        let snapshot = self
            .pending
            .borrow()
            .clone()
            .unwrap_or_else(|| RestorePoint {
                focus_epoch: self.focus_epoch.get(),
                focused: focused.clone(),
                neighbors: focused_neighbors.clone(),
                positions: positions.clone(),
                x: self.scroll.hadjustment().value(),
            });
        let mut snapshot = snapshot;
        if snapshot.focus_epoch != self.focus_epoch.get() {
            snapshot.focus_epoch = self.focus_epoch.get();
            snapshot.x = self.scroll.hadjustment().value();
            if let Some((list, _, _)) = focused_neighbors.as_ref() {
                if let Some(index) = self.columns.iter().position(|column| &column.list == list) {
                    snapshot.positions[index] = positions[index].clone();
                }
            }
            snapshot.focused = focused;
            snapshot.neighbors = focused_neighbors;
        }
        *self.pending.borrow_mut() = Some(snapshot.clone());
        let board = build_board(tasks, Utc::now());
        let current = board
            .iter()
            .flat_map(|col| col.cards.iter().map(|card| (card.id.clone(), card.clone())))
            .collect::<std::collections::HashMap<_, _>>();
        self.reconciling.set(true);
        let mut rows = self.rows.borrow_mut();
        rows.retain(|id, widgets| {
            if current.contains_key(id) {
                return true;
            }
            if let Some(parent) = widgets.row.parent().and_downcast::<gtk4::ListBox>() {
                parent.remove(&widgets.row);
            }
            false
        });
        for (col, view) in self.columns.iter().zip(board) {
            col.header
                .set_text(&format!("{} · {}", view.column.title(), view.total_count));
            col.notice.set_visible(view.cards.len() < view.total_count);
            col.notice.set_text(&format!(
                "Showing latest {} of {}",
                view.cards.len(),
                view.total_count
            ));
            for (index, card) in view.cards.iter().enumerate() {
                let widgets = rows
                    .entry(card.id.clone())
                    .or_insert_with(|| CardWidgets::new(card));
                widgets.update(card);
                if widgets.row.parent().as_ref() != Some(col.list.upcast_ref())
                    || widgets.row.index() != index as i32
                {
                    if let Some(parent) = widgets.row.parent().and_downcast::<gtk4::ListBox>() {
                        parent.remove(&widgets.row);
                    }
                    col.list.insert(&widgets.row, index as i32);
                }
            }
        }
        *self.cards.borrow_mut() = current;
        self.stack
            .set_visible_child_name(if tasks.is_empty() { "empty" } else { "board" });
        // Restore identity immediately so GTK does not choose its first row
        // while another event arrives before the next allocation.
        if snapshot.focused.is_some() {
            if let Some(row) = focus_target(&rows, &snapshot) {
                row.grab_focus();
            }
        }
        drop(rows);
        self.reconciling.set(false);
        if !self.dialog.is_mapped() {
            self.pending.borrow_mut().take();
            return;
        }
        let focus_epoch = self.focus_epoch.clone();
        let focus_ticket = focus_epoch.get();
        let revision = self.revision.get().wrapping_add(1);
        self.revision.set(revision);
        let generation = self.revision.clone();
        let dialog = self.dialog.downgrade();
        let content = self.content.clone();
        let rows = self.rows.clone();
        let pending = self.pending.clone();
        let closing = self.closing.clone();
        let horizontal = self.scroll.hadjustment();
        let columns = self
            .columns
            .iter()
            .map(|col| (col.list.clone(), col.scroll.vadjustment()))
            .collect::<Vec<_>>();
        // Tick twice: the first frame allocates changed rows, the next reads them.
        let allocated = std::cell::Cell::new(false);
        self.stack.add_tick_callback(move |_, _| {
            if generation.get() != revision || closing.get() {
                return gtk4::glib::ControlFlow::Break;
            }
            if !allocated.replace(true) {
                return gtk4::glib::ControlFlow::Continue;
            }
            let Some(dialog) = dialog.upgrade().filter(|dialog| dialog.is_mapped()) else {
                pending.borrow_mut().take();
                return gtk4::glib::ControlFlow::Break;
            };
            pending.borrow_mut().take();
            let user_focus = focus_epoch.get() != focus_ticket
                && dialog
                    .focus()
                    .is_some_and(|widget| !widget.is::<gtk4::ListBox>());
            let current_focus = dialog.focus();
            let rows = rows.borrow();
            let focused_row = focus_target(&rows, &snapshot);
            let RestorePoint {
                x,
                positions,
                focused,
                ..
            } = &snapshot;
            if !user_focus {
                horizontal.set_value(*x);
            }
            for ((list, adjustment), (old, anchor)) in columns.iter().zip(positions.iter()) {
                // New navigation owns its column; unrelated columns retain anchors.
                if user_focus
                    && current_focus
                        .as_ref()
                        .is_some_and(|focus| focus.is_ancestor(list))
                {
                    continue;
                }
                let value = if *old <= 0.0 {
                    0.0
                } else {
                    anchor
                        .as_ref()
                        .and_then(|(id, offset)| {
                            let mut index = 0;
                            while let Some(row) = list.row_at_index(index) {
                                if row.widget_name().as_str() == id {
                                    return row
                                        .compute_bounds(list)
                                        .map(|bounds| f64::from(bounds.y()) - *offset);
                                }
                                index += 1;
                            }
                            None
                        })
                        .unwrap_or(*old)
                };
                adjustment.set_value(
                    value.clamp(0.0, (adjustment.upper() - adjustment.page_size()).max(0.0)),
                );
            }
            if user_focus {
                return gtk4::glib::ControlFlow::Break;
            }
            if let Some(row) = focused_row {
                if row.parent().is_some() {
                    let reveal = snapshot.neighbors.as_ref().is_some_and(|(list, _, _)| {
                        row.parent().as_ref() != Some(list.upcast_ref())
                    }) || focused
                        .as_ref()
                        .is_some_and(|id| row.widget_name().as_str() != id);
                    row.grab_focus();
                    if reveal {
                        if let (Some(bounds), Some(content_bounds)) = (
                            row.compute_bounds(&content),
                            content.compute_bounds(&content),
                        ) {
                            // CSS padding places the content origin inside its border box.
                            let left = f64::from(bounds.x() - content_bounds.x());
                            horizontal.clamp_page(left, left + f64::from(bounds.width()));
                        }
                        if let Some(list) = row.parent().and_downcast::<gtk4::ListBox>() {
                            if let Some(vertical) = list
                                .ancestor(gtk4::ScrolledWindow::static_type())
                                .and_downcast::<gtk4::ScrolledWindow>()
                            {
                                if let Some(bounds) = row.compute_bounds(&list) {
                                    vertical.vadjustment().clamp_page(
                                        f64::from(bounds.y()),
                                        f64::from(bounds.y() + bounds.height()),
                                    );
                                }
                            }
                        }
                    }
                }
            } else if focused.is_some() {
                dialog.set_focus(None::<&gtk4::Widget>);
                if let Some(body) = dialog.child() {
                    body.child_focus(gtk4::DirectionType::TabForward);
                }
            }
            gtk4::glib::ControlFlow::Break
        });
    }
}

impl Drop for Board {
    fn drop(&mut self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        self.pending.borrow_mut().take();
    }
}

/// Present a board. Activation resolves the current pane target by task id.
pub fn present(
    window: &libadwaita::ApplicationWindow,
    tasks: &[Task],
    on_closed: impl FnOnce(&libadwaita::Dialog, Option<String>) + 'static,
) -> Board {
    let dialog = libadwaita::Dialog::new();
    dialog.set_title("Task Board");
    dialog.set_content_width(1200);
    dialog.set_content_height(600);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    body.append(&libadwaita::HeaderBar::new());
    let stack = gtk4::Stack::new();
    stack.set_vexpand(true);
    let empty = libadwaita::StatusPage::new();
    empty.set_icon_name(Some("utilities-terminal-symbolic"));
    empty.set_title("No tasks yet");
    empty.set_description(Some("Tasks run by agents will appear here."));
    stack.add_named(&empty, Some("empty"));
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    container.add_css_class("board-container");
    container.set_vexpand(true);
    container.set_hexpand(true);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Never);
    scroll.set_child(Some(&container));
    stack.add_named(&scroll, Some("board"));
    body.append(&stack);
    let cards: Rc<RefCell<std::collections::HashMap<String, TaskCardView>>> = Rc::default();
    let chosen: Rc<RefCell<Option<String>>> = Rc::default();
    let mut columns = Vec::new();
    for column in [
        BoardColumn::Working,
        BoardColumn::NeedsYou,
        BoardColumn::InReview,
        BoardColumn::ReadyToMerge,
        BoardColumn::Done,
    ] {
        let col = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        col.add_css_class("board-column");
        if column == BoardColumn::Done {
            col.add_css_class("board-column-done");
        }
        col.set_hexpand(true);
        col.set_vexpand(true);
        let header = gtk4::Label::new(None);
        header.add_css_class("board-column-header");
        header.set_halign(gtk4::Align::Start);
        col.append(&header);
        let notice = gtk4::Label::new(None);
        notice.add_css_class("board-card-meta");
        notice.set_wrap(true);
        notice.set_xalign(0.0);
        col.append(&notice);
        let vertical = gtk4::ScrolledWindow::new();
        vertical.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        vertical.set_vexpand(true);
        vertical.set_hexpand(true);
        let list = gtk4::ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::None);
        list.add_css_class("board-column-list");
        let current = cards.clone();
        let chosen = chosen.clone();
        let weak = dialog.downgrade();
        list.connect_row_activated(move |_, row| {
            let pane = current
                .borrow()
                .get(row.widget_name().as_str())
                .and_then(|card| card.pane_id.clone());
            if let Some(pane) = pane {
                chosen.replace(Some(pane));
                if let Some(dialog) = weak.upgrade() {
                    dialog.close();
                }
            }
        });
        vertical.set_child(Some(&list));
        col.append(&vertical);
        container.append(&col);
        columns.push(ColumnWidgets {
            header,
            notice,
            scroll: vertical,
            list,
        });
    }
    dialog.set_child(Some(&body));
    let closing = Rc::new(std::cell::Cell::new(false));
    let closed = closing.clone();
    let on_closed = RefCell::new(Some(on_closed));
    dialog.connect_closed(move |dialog| {
        closed.set(true);
        if let Some(cb) = on_closed.borrow_mut().take() {
            cb(dialog, chosen.take());
        }
    });
    let focus_epoch = Rc::new(std::cell::Cell::new(0_u64));
    let reconciling = Rc::new(std::cell::Cell::new(false));
    let epoch = focus_epoch.clone();
    let updating = reconciling.clone();
    dialog.connect_focus_widget_notify(move |_| {
        if !updating.get() {
            epoch.set(epoch.get().wrapping_add(1));
        }
    });
    let board = Board {
        dialog,
        stack,
        scroll,
        content: container,
        columns,
        rows: Rc::default(),
        pending: Rc::default(),
        cards,
        revision: Rc::default(),
        focus_epoch,
        reconciling,
        closing,
    };
    board.update(tasks);
    board.dialog.present(Some(window));
    board
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use signaltty_core::{Contract, Disposition, Relationship, TaskPr};
    use std::path::PathBuf;

    fn make_task(
        id: &str,
        label: &str,
        state: TaskState,
        outcome: DispositionOutcome,
        updated_at: DateTime<Utc>,
    ) -> Task {
        Task {
            id: id.to_string(),
            context_id: "ctx-test".to_string(),
            parent_task_id: None,
            pane_id: Some("pane-test".to_string()),
            parent_pane_id: None,
            root_pane_id: None,
            relationship: Relationship::Subagent,
            label: label.to_string(),
            contract: Contract {
                objective: "objective text".to_string(),
                constraints: None,
                acceptance_criteria: None,
                output_format: None,
            },
            agent: Some("codex".to_string()),
            source_repo: PathBuf::from("/repo"),
            target_branch: None,
            worktree_path: PathBuf::from("/worktree"),
            branch: "orch/feat-1".to_string(),
            preexisting_branch: false,
            base_ref: "main".to_string(),
            base_sha: "abcdef".to_string(),
            state,
            result: None,
            disposition: Disposition {
                outcome,
                target_ref: None,
                merged_sha: None,
                branch_deleted: None,
                at: None,
            },
            pr: None,
            status_reason: None,
            finish_error: None,
            worker_pid: None,
            worker_cmd: None,
            client_request_id: None,
            created_at: updated_at,
            updated_at,
        }
    }

    #[test]
    fn test_board_column_rules() {
        let base_time = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();

        // 1. Working: Pending | Working with outcome == None
        let task_pending = make_task(
            "t1",
            "Pending",
            TaskState::Pending,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_pending), BoardColumn::Working);

        let task_working = make_task(
            "t2",
            "Working",
            TaskState::Working,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_working), BoardColumn::Working);

        // 2. NeedsYou: InputRequired | Failed | Rejected when outcome == None
        let task_input = make_task(
            "t3",
            "Input",
            TaskState::InputRequired,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_input), BoardColumn::NeedsYou);

        let task_failed = make_task(
            "t4",
            "Failed",
            TaskState::Failed,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_failed), BoardColumn::NeedsYou);

        let task_rejected = make_task(
            "t5",
            "Rejected",
            TaskState::Rejected,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_rejected), BoardColumn::NeedsYou);

        // 3. InReview: Completed when outcome == None
        let task_completed = make_task(
            "t6",
            "Completed",
            TaskState::Completed,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_completed), BoardColumn::InReview);

        // 4. Done: Canceled regardless of outcome
        let task_canceled = make_task(
            "t7",
            "Canceled",
            TaskState::Canceled,
            DispositionOutcome::None,
            base_time,
        );
        assert_eq!(board_column(&task_canceled), BoardColumn::Done);

        // 5. Done: Any state when outcome is Merged or Discarded
        let task_merged_completed = make_task(
            "t8",
            "Merged",
            TaskState::Completed,
            DispositionOutcome::Merged,
            base_time,
        );
        assert_eq!(board_column(&task_merged_completed), BoardColumn::Done);

        let task_discarded_failed = make_task(
            "t9",
            "Discarded",
            TaskState::Failed,
            DispositionOutcome::Discarded,
            base_time,
        );
        assert_eq!(board_column(&task_discarded_failed), BoardColumn::Done);

        let task_merged_working = make_task(
            "t10",
            "Merged Working",
            TaskState::Working,
            DispositionOutcome::Merged,
            base_time,
        );
        assert_eq!(board_column(&task_merged_working), BoardColumn::Done);

        let task_discarded_input = make_task(
            "t11",
            "Discarded Input",
            TaskState::InputRequired,
            DispositionOutcome::Discarded,
            base_time,
        );
        assert_eq!(board_column(&task_discarded_input), BoardColumn::Done);
    }

    #[test]
    fn test_relative_age_formatting() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();

        assert_eq!(relative_age(now, now), "now");
        assert_eq!(relative_age(now - Duration::seconds(30), now), "now");
        assert_eq!(relative_age(now + Duration::seconds(10), now), "now"); // clock skew
        assert_eq!(relative_age(now - Duration::minutes(3), now), "3m");
        assert_eq!(relative_age(now - Duration::minutes(59), now), "59m");
        assert_eq!(relative_age(now - Duration::hours(2), now), "2h");
        assert_eq!(relative_age(now - Duration::hours(23), now), "23h");
        assert_eq!(relative_age(now - Duration::days(1), now), "1d");
        assert_eq!(relative_age(now - Duration::days(5), now), "5d");
    }

    #[test]
    fn test_column_sort_order() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let t_old = make_task(
            "t_old",
            "Older Task",
            TaskState::Working,
            DispositionOutcome::None,
            now - Duration::minutes(20),
        );
        let t_new = make_task(
            "t_new",
            "Newer Task",
            TaskState::Working,
            DispositionOutcome::None,
            now - Duration::minutes(2),
        );
        let t_mid = make_task(
            "t_mid",
            "Mid Task",
            TaskState::Working,
            DispositionOutcome::None,
            now - Duration::minutes(10),
        );

        let board = build_board(&[t_old, t_new, t_mid], now);
        let working_col = &board[0];
        assert_eq!(working_col.column, BoardColumn::Working);
        assert_eq!(working_col.cards.len(), 3);
        assert_eq!(working_col.cards[0].id, "t_new");
        assert_eq!(working_col.cards[1].id, "t_mid");
        assert_eq!(working_col.cards[2].id, "t_old");
    }

    #[test]
    fn test_done_column_truncation_to_20() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let mut tasks = Vec::new();
        for i in 0..25 {
            tasks.push(make_task(
                &format!("t_{i:02}"),
                &format!("Done Task {i}"),
                TaskState::Completed,
                DispositionOutcome::Merged,
                now - Duration::minutes(i as i64),
            ));
        }

        let board = build_board(&tasks, now);
        let done_col = &board[4];
        assert_eq!(done_col.column, BoardColumn::Done);
        assert_eq!(done_col.total_count, 25);
        assert_eq!(done_col.cards.len(), 20);
        // The first card should be the latest (t_00)
        assert_eq!(done_col.cards[0].id, "t_00");
        // The 20th card should be t_19
        assert_eq!(done_col.cards[19].id, "t_19");
    }

    #[test]
    fn test_task_card_view_details() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let mut task = make_task(
            "task-abc12345",
            "  Fix memory leak  ",
            TaskState::Completed,
            DispositionOutcome::None,
            now - Duration::minutes(5),
        );
        task.agent = Some("codex".to_string());
        task.branch = "feat/leak".to_string();

        let card = task_to_card(&task, now);
        assert_eq!(card.label, "Fix memory leak");
        assert_eq!(card.agent, "codex");
        assert_eq!(card.branch, "feat/leak");
        assert_eq!(card.state_word, "Completed");
        assert_eq!(card.css_class, "task-completed");
        assert_eq!(card.relative_age, "5m");
        assert_eq!(card.subtitle(), "codex · feat/leak · 5m");

        // When label is empty, fall back to id tail
        task.label = "   ".to_string();
        let card2 = task_to_card(&task, now);
        assert_eq!(card2.label, "abc12345");

        // When agent is None, subtitle omits agent cleanly
        task.agent = None;
        let card3 = task_to_card(&task, now);
        assert_eq!(card3.subtitle(), "feat/leak · 5m");
    }

    #[test]
    fn test_board_column_titles() {
        assert_eq!(BoardColumn::Working.title(), "Working");
        assert_eq!(BoardColumn::NeedsYou.title(), "Needs you");
        assert_eq!(BoardColumn::InReview.title(), "In review");
        assert_eq!(BoardColumn::ReadyToMerge.title(), "Ready to merge");
        assert_eq!(BoardColumn::Done.title(), "Done");
    }

    #[test]
    fn test_board_column_with_pr() {
        let base_time = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();

        let make_pr_task =
            |state: TaskState, pr_state: PrState, checks: PrChecks, review: PrReview| {
                let mut t = make_task("t", "PR Task", state, DispositionOutcome::None, base_time);
                t.pr = Some(TaskPr {
                    number: 42,
                    url: "https://github.com/org/repo/pull/42".to_string(),
                    state: pr_state,
                    checks,
                    review,
                    checked_at: None,
                });
                t
            };

        // 1. Merged or Closed -> Done
        let t_merged = make_pr_task(
            TaskState::Completed,
            PrState::Merged,
            PrChecks::Passing,
            PrReview::Approved,
        );
        assert_eq!(board_column(&t_merged), BoardColumn::Done);
        let t_closed = make_pr_task(
            TaskState::Completed,
            PrState::Closed,
            PrChecks::None,
            PrReview::None,
        );
        assert_eq!(board_column(&t_closed), BoardColumn::Done);

        // 2. Open with failing checks or changes requested -> NeedsYou
        let t_failing = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Failing,
            PrReview::Approved,
        );
        assert_eq!(board_column(&t_failing), BoardColumn::NeedsYou);
        let t_changes = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Passing,
            PrReview::ChangesRequested,
        );
        assert_eq!(board_column(&t_changes), BoardColumn::NeedsYou);

        // 3. Open with checks passing/none and review approved/none -> ReadyToMerge (once refreshed)
        let mut t_ready1 = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Passing,
            PrReview::Approved,
        );
        t_ready1.pr.as_mut().unwrap().checked_at = Some(base_time);
        assert_eq!(board_column(&t_ready1), BoardColumn::ReadyToMerge);

        let mut t_ready2 = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::None,
            PrReview::None,
        );
        t_ready2.pr.as_mut().unwrap().checked_at = Some(base_time);
        assert_eq!(board_column(&t_ready2), BoardColumn::ReadyToMerge);

        let mut t_ready3 = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Passing,
            PrReview::None,
        );
        t_ready3.pr.as_mut().unwrap().checked_at = Some(base_time);
        assert_eq!(board_column(&t_ready3), BoardColumn::ReadyToMerge);

        let mut t_ready4 = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::None,
            PrReview::Approved,
        );
        t_ready4.pr.as_mut().unwrap().checked_at = Some(base_time);
        assert_eq!(board_column(&t_ready4), BoardColumn::ReadyToMerge);

        // 4. Open with pending checks or review required -> InReview
        let t_pending = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Pending,
            PrReview::None,
        );
        assert_eq!(board_column(&t_pending), BoardColumn::InReview);
        let t_rev_req = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Passing,
            PrReview::ReviewRequired,
        );
        assert_eq!(board_column(&t_rev_req), BoardColumn::InReview);

        // 5. Open PR never refreshed (checked_at == None) -> InReview
        let t_unrefreshed = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::None,
            PrReview::None,
        );
        assert_eq!(board_column(&t_unrefreshed), BoardColumn::InReview);

        // 6. Terminal disposition Discarded wins over open PR -> Done
        let mut t_discarded_pr = make_pr_task(
            TaskState::Completed,
            PrState::Open,
            PrChecks::Passing,
            PrReview::Approved,
        );
        t_discarded_pr.disposition.outcome = DispositionOutcome::Discarded;
        assert_eq!(board_column(&t_discarded_pr), BoardColumn::Done);

        // 7. Canceled state wins over open PR -> Done
        let t_canceled_pr = make_pr_task(
            TaskState::Canceled,
            PrState::Open,
            PrChecks::Passing,
            PrReview::Approved,
        );
        assert_eq!(board_column(&t_canceled_pr), BoardColumn::Done);
    }

    #[test]
    fn test_task_card_view_subtitle_with_pr() {
        let now = Utc.with_ymd_and_hms(2026, 10, 4, 12, 0, 0).unwrap();
        let mut task = make_task(
            "task-abc12345",
            "PR Feature",
            TaskState::Completed,
            DispositionOutcome::None,
            now - Duration::minutes(5),
        );
        task.agent = Some("codex".to_string());
        task.branch = "feat/pr".to_string();

        // No PR
        let card = task_to_card(&task, now);
        assert_eq!(card.subtitle(), "codex · feat/pr · 5m");

        // PR with checks failing
        task.pr = Some(TaskPr {
            number: 101,
            url: "url".to_string(),
            state: PrState::Open,
            checks: PrChecks::Failing,
            review: PrReview::None,
            checked_at: None,
        });
        let card = task_to_card(&task, now);
        assert_eq!(
            card.subtitle(),
            "codex · feat/pr · 5m · #101 · checks failing"
        );

        // PR with checks passing
        task.pr.as_mut().unwrap().checks = PrChecks::Passing;
        let card = task_to_card(&task, now);
        assert_eq!(
            card.subtitle(),
            "codex · feat/pr · 5m · #101 · checks passing"
        );

        // PR with checks pending
        task.pr.as_mut().unwrap().checks = PrChecks::Pending;
        let card = task_to_card(&task, now);
        assert_eq!(
            card.subtitle(),
            "codex · feat/pr · 5m · #101 · checks pending"
        );

        // PR with checks none
        task.pr.as_mut().unwrap().checks = PrChecks::None;
        let card = task_to_card(&task, now);
        assert_eq!(card.subtitle(), "codex · feat/pr · 5m · #101");
    }
}
