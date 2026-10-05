//! Task chip for a worker pane: a "Task" prefix, optional label, state
//! word, and a tooltip.
//!
//! The prefix keeps the chip from reading as a second lifecycle word
//! next to "Idle" or "Working" on the pane. A label that repeats the
//! adjacent workspace or pane name is omitted; it stays in the tooltip
//! and the accessible name. Pure mapping and the pane-keyed cache live
//! here so headless tests cover present / absent / update. The widget
//! only paints that view. Colours are the theme tokens in
//! `data/style.css` (`task-*` classes).

use std::collections::HashMap;

use gtk4::prelude::*;
use serde_json::Value;
use signaltty_core::{Task, TaskState};
use signaltty_proto::event;

/// CSS classes swapped on the chip. One is set at a time.
pub const TASK_CHIP_CLASSES: [&str; 7] = [
    "task-pending",
    "task-working",
    "task-input",
    "task-completed",
    "task-failed",
    "task-canceled",
    "task-rejected",
];

/// Visible category so the chip is a task, not another lifecycle word.
pub const TASK_KIND: &str = "Task";

/// What the chip shows. Built without GTK so tests can pin every state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskChipView {
    pub kind: &'static str,
    pub label: String,
    /// False when `label` repeats the workspace or pane name beside the
    /// chip. The label still fills `tooltip` and `accessible_name`.
    pub show_label: bool,
    /// `from <orchestrator>`, only when the parent name stays short.
    pub lineage: Option<String>,
    pub state: &'static str,
    pub css_class: &'static str,
    pub tooltip: String,
    pub accessible_name: String,
}

pub fn state_word(state: TaskState) -> &'static str {
    match state {
        TaskState::Pending => "Pending",
        TaskState::Working => "Working",
        TaskState::InputRequired => "Input",
        TaskState::Completed => "Completed",
        TaskState::Failed => "Failed",
        TaskState::Canceled => "Canceled",
        TaskState::Rejected => "Rejected",
    }
}

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::InputRequired => "Input required",
        other => state_word(other),
    }
}

pub fn state_class(state: TaskState) -> &'static str {
    match state {
        TaskState::Pending => "task-pending",
        TaskState::Working => "task-working",
        TaskState::InputRequired => "task-input",
        TaskState::Completed => "task-completed",
        TaskState::Failed => "task-failed",
        TaskState::Canceled => "task-canceled",
        TaskState::Rejected => "task-rejected",
    }
}

/// Map one task onto the chip. `parent_label` is the orchestrator pane's
/// label or title; a long or identical name stays in the tooltip only.
/// `context_name` is the workspace or pane name drawn beside this chip.
/// When the task label equals that name, or is contained in it, the
/// chip shows the state without repeating the name.
pub fn task_chip(
    task: &Task,
    parent_label: Option<&str>,
    context_name: Option<&str>,
) -> TaskChipView {
    let label = display_label(task);
    let state = state_word(task.state);
    let lineage = lineage_phrase(&label, parent_label);
    let show_label = match context_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => !label_repeats_name(&label, name),
        None => true,
    };
    let mut accessible_name = format!("{TASK_KIND} {label}, {}", state_name(task.state));
    if let Some(line) = &lineage {
        accessible_name.push_str(", ");
        accessible_name.push_str(line);
    }
    TaskChipView {
        kind: TASK_KIND,
        label,
        show_label,
        lineage,
        state,
        css_class: state_class(task.state),
        tooltip: tooltip(task, parent_label),
        accessible_name,
    }
}

/// The label repeats the name beside the chip when it is that name or
/// a piece of it (`parser` beside `fix-parser`). Comparison is
/// case-insensitive. The other direction does not match: a longer
/// label than the name is still shown.
fn label_repeats_name(label: &str, name: &str) -> bool {
    let label = label.trim();
    let name = name.trim();
    if label.is_empty() || name.is_empty() {
        return false;
    }
    name.to_lowercase().contains(&label.to_lowercase())
}

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

/// Visible lineage only when it reads as a short name, not a path or the
/// task's own label.
fn lineage_phrase(task_label: &str, parent_label: Option<&str>) -> Option<String> {
    let parent = parent_label.map(str::trim).filter(|s| !s.is_empty())?;
    if parent.eq_ignore_ascii_case(task_label.trim()) {
        return None;
    }
    if parent.chars().count() > 24 || parent.contains(['\n', '/']) {
        return None;
    }
    Some(format!("from {parent}"))
}

fn tooltip(task: &Task, parent_label: Option<&str>) -> String {
    let mut lines = Vec::new();
    let label = display_label(task);
    if !label.is_empty() {
        lines.push(label);
    }
    let objective = collapse(&task.contract.objective);
    if !objective.is_empty() {
        lines.push(clip_chars(&objective, 240));
    }
    let branch = task.branch.trim();
    if !branch.is_empty() {
        lines.push(format!("branch {branch}"));
    }
    let base = if !task.base_sha.trim().is_empty() {
        short_sha(task.base_sha.trim())
    } else {
        task.base_ref.trim().to_string()
    };
    if !base.is_empty() {
        lines.push(format!("base {base}"));
    }
    if let Some(parent) = parent_label.map(str::trim).filter(|s| !s.is_empty()) {
        lines.push(format!("from {}", clip_chars(&collapse(parent), 80)));
    }
    if lines.is_empty() {
        state_word(task.state).to_string()
    } else {
        lines.join("\n")
    }
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(12).collect()
}

/// Tasks keyed by worker pane. Seeded from `task.list`, then updated
/// in place from `task.created` / `task.updated`.
#[derive(Debug, Default)]
pub struct TaskIndex {
    by_id: HashMap<String, Task>,
    by_pane: HashMap<String, Task>,
    order: Vec<String>,
    epoch: u64,
}

impl TaskIndex {
    pub fn seed_ticket(&self) -> u64 {
        self.epoch
    }

    /// Apply a `task.list` body. A list that started before a newer
    /// event fills in tasks the event did not already mention, and
    /// does not resurrect a pane a newer event cleared.
    pub fn complete_seed(&mut self, ticket: u64, tasks: Vec<Task>) {
        if self.epoch == ticket {
            self.by_id.clear();
            self.by_pane.clear();
            self.order.clear();
            for task in tasks {
                self.remember(task);
            }
        } else {
            for task in tasks {
                if !self.by_id.contains_key(&task.id) {
                    self.remember(task);
                }
            }
        }
    }

    /// `task.created` and `task.updated` carry the full task.
    /// `task.result` does not change the chip; the paired update does.
    pub fn apply_event(&mut self, name: &str, payload: &Value) -> bool {
        if name != event::TASK_CREATED && name != event::TASK_UPDATED {
            return false;
        }
        let Some(task) = payload
            .get("task")
            .and_then(|v| serde_json::from_value::<Task>(v.clone()).ok())
        else {
            return false;
        };
        self.epoch = self.epoch.wrapping_add(1);
        self.remember(task);
        true
    }

    pub fn get_by_id(&self, task_id: &str) -> Option<&Task> {
        self.by_id.get(task_id)
    }

    #[cfg(test)]
    fn get(&self, pane_id: &str) -> Option<&Task> {
        self.by_pane.get(pane_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Task> {
        self.order.iter().filter_map(|id| self.by_pane.get(id))
    }

    fn remember(&mut self, task: Task) {
        if let Some(old) = self
            .by_id
            .get(&task.id)
            .and_then(|prev| prev.pane_id.clone())
        {
            // Only drop the pane entry while it still belongs to this task: a
            // later task may own the pane now.
            if task.pane_id.as_deref() != Some(old.as_str())
                && self.by_pane.get(&old).is_some_and(|t| t.id == task.id)
            {
                self.by_pane.remove(&old);
                self.order.retain(|id| id != &old);
            }
        }
        if let Some(pane_id) = task.pane_id.clone() {
            if !self.order.iter().any(|id| id == &pane_id) {
                self.order.push(pane_id.clone());
            }
            self.by_pane.insert(pane_id, task.clone());
        }
        self.by_id.insert(task.id.clone(), task);
    }
}

/// `task.list` result field. `None` when the body has no array, so a
/// missing key does not wipe a cache.
pub fn parse_task_list(value: &Value) -> Option<Vec<Task>> {
    let items = value.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|v| serde_json::from_value::<Task>(v.clone()).ok())
            .collect(),
    )
}

fn set_state_class(widget: &impl IsA<gtk4::Widget>, class: &str) {
    for name in TASK_CHIP_CLASSES {
        if name != class {
            widget.remove_css_class(name);
        }
    }
    widget.add_css_class(class);
}

fn set_label(label: &gtk4::Label, text: &str) {
    if label.text().as_str() != text {
        label.set_text(text);
    }
}

/// Compact chip. Hidden when the pane has no task; state changes
/// rewrite the same labels.
pub struct TaskChipWidget {
    pub root: gtk4::Box,
    kind: gtk4::Label,
    label: gtk4::Label,
    lineage: gtk4::Label,
    state: gtk4::Label,
}

impl TaskChipWidget {
    pub fn new() -> TaskChipWidget {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        root.add_css_class("task-chip");
        root.set_valign(gtk4::Align::Center);
        root.set_halign(gtk4::Align::Start);
        root.set_hexpand(false);
        root.set_visible(false);
        root.set_accessible_role(gtk4::AccessibleRole::Group);

        let text = || {
            let label = gtk4::Label::new(None);
            label.set_accessible_role(gtk4::AccessibleRole::Presentation);
            label.set_hexpand(false);
            label.set_xalign(0.0);
            label
        };
        let kind = text();
        kind.add_css_class("task-chip-kind");
        let label = text();
        label.add_css_class("task-chip-label");
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(18);
        let lineage = text();
        lineage.add_css_class("task-chip-lineage");
        lineage.set_visible(false);
        let state = text();
        state.add_css_class("task-chip-state");
        root.append(&kind);
        root.append(&label);
        root.append(&lineage);
        root.append(&state);
        TaskChipWidget {
            root,
            kind,
            label,
            lineage,
            state,
        }
    }

    pub fn set(&self, view: Option<&TaskChipView>) {
        let Some(view) = view else {
            self.root.set_visible(false);
            return;
        };
        set_state_class(&self.root, view.css_class);
        set_label(&self.kind, view.kind);
        if view.show_label {
            set_label(&self.label, &view.label);
            self.label.set_visible(true);
        } else {
            self.label.set_visible(false);
        }
        match &view.lineage {
            Some(line) => {
                set_label(&self.lineage, line);
                self.lineage.set_visible(true);
            }
            None => self.lineage.set_visible(false),
        }
        set_label(&self.state, view.state);
        self.root.set_tooltip_text(Some(&view.tooltip));
        self.root
            .update_property(&[gtk4::accessible::Property::Label(&view.accessible_name)]);
        self.root.set_visible(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use signaltty_core::{Contract, Disposition, Relationship, TaskState};
    use std::path::PathBuf;

    fn task(state: TaskState) -> Task {
        let now = Utc::now();
        Task {
            id: "task_abc12345".into(),
            context_id: "tctx_1".into(),
            parent_task_id: None,
            pane_id: Some("pane_worker".into()),
            parent_pane_id: Some("pane_orch".into()),
            root_pane_id: Some("pane_orch".into()),
            relationship: Relationship::Subagent,
            label: "fix-parser".into(),
            contract: Contract::new("Tighten the parser").unwrap(),
            agent: None,
            source_repo: PathBuf::from("/tmp/repo"),
            target_branch: Some("main".into()),
            worktree_path: PathBuf::from("/tmp/wt"),
            branch: "fix/parser".into(),
            preexisting_branch: false,
            base_ref: "HEAD".into(),
            base_sha: "0123456789abcdef".into(),
            state,
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
        }
    }

    fn view(state: TaskState, parent: Option<&str>) -> TaskChipView {
        task_chip(&task(state), parent, None)
    }

    #[test]
    fn every_state_maps_to_a_word_and_class() {
        let cases = [
            (TaskState::Pending, "Pending", "task-pending"),
            (TaskState::Working, "Working", "task-working"),
            (TaskState::InputRequired, "Input", "task-input"),
            (TaskState::Completed, "Completed", "task-completed"),
            (TaskState::Failed, "Failed", "task-failed"),
            (TaskState::Canceled, "Canceled", "task-canceled"),
            (TaskState::Rejected, "Rejected", "task-rejected"),
        ];
        for (state, word, class) in cases {
            let chip = view(state, None);
            assert_eq!(chip.state, word);
            assert_eq!(chip.css_class, class);
            assert!(TASK_CHIP_CLASSES.contains(&chip.css_class));
            assert_eq!(chip.label, "fix-parser");
        }
    }

    #[test]
    fn input_uses_the_warning_token_and_completed_uses_success() {
        let css = include_str!("../data/style.css");
        assert!(css.contains(".task-chip.task-input { color: var(--warning-color);"));
        assert!(css.contains(".task-chip.task-completed { color: var(--success-color);"));
        for class in TASK_CHIP_CLASSES {
            assert!(css.contains(&format!(".task-chip.{class}")), "{class}");
        }
        assert!(css.contains(".high-contrast .task-chip"));
        assert!(css.contains(".task-chip-kind"));
        assert!(css.contains(".high-contrast .task-chip-kind"));
    }

    #[test]
    fn tooltip_carries_objective_branch_and_base() {
        let chip = view(TaskState::Working, None);
        assert!(
            chip.tooltip.contains("Tighten the parser"),
            "{}",
            chip.tooltip
        );
        assert!(
            chip.tooltip.contains("branch fix/parser"),
            "{}",
            chip.tooltip
        );
        assert!(
            chip.tooltip.contains("base 0123456789ab"),
            "{}",
            chip.tooltip
        );
        assert_eq!(chip.accessible_name, "Task fix-parser, Working");
        assert!(chip.tooltip.starts_with("fix-parser\n"), "{}", chip.tooltip);
    }

    #[test]
    fn input_required_accessible_name_says_the_full_state() {
        let chip = view(TaskState::InputRequired, None);
        assert_eq!(chip.state, "Input");
        assert_eq!(chip.accessible_name, "Task fix-parser, Input required");
    }

    #[test]
    fn label_hides_when_it_repeats_the_adjacent_name() {
        let same = task_chip(&task(TaskState::Working), None, Some("fix-parser"));
        assert!(!same.show_label);
        assert_eq!(same.label, "fix-parser");
        assert_eq!(same.kind, "Task");
        assert_eq!(same.state, "Working");
        assert_eq!(same.accessible_name, "Task fix-parser, Working");
        assert!(same.tooltip.contains("fix-parser"), "{}", same.tooltip);

        let contained = task_chip(
            &task(TaskState::Working),
            None,
            Some("workspace fix-parser"),
        );
        assert!(!contained.show_label);

        let case = task_chip(&task(TaskState::Working), None, Some("Fix-Parser"));
        assert!(!case.show_label);

        let different = task_chip(&task(TaskState::Working), None, Some("worker"));
        assert!(different.show_label);
        assert_eq!(different.label, "fix-parser");

        // A longer label is not "contained in" the shorter name.
        let shorter = task_chip(&task(TaskState::Working), None, Some("parser"));
        assert!(shorter.show_label);

        let unnamed = task_chip(&task(TaskState::Working), None, Some("  "));
        assert!(unnamed.show_label);
    }

    #[test]
    fn lineage_shows_a_short_orchestrator_and_hides_a_long_one() {
        let short = view(TaskState::Working, Some("orchestrator"));
        assert_eq!(short.lineage.as_deref(), Some("from orchestrator"));
        assert!(short.accessible_name.contains("from orchestrator"));
        assert!(short.tooltip.contains("from orchestrator"));

        let long = view(TaskState::Working, Some(&"orchestrator-".repeat(6)));
        assert!(long.lineage.is_none());
        assert!(long.tooltip.contains("from orchestrator-"));

        let path = view(TaskState::Working, Some("/home/work/repo"));
        assert!(path.lineage.is_none());
        assert!(path.tooltip.contains("from /home/work/repo"));

        let same = view(TaskState::Working, Some("fix-parser"));
        assert!(same.lineage.is_none());
    }

    #[test]
    fn chip_is_absent_until_a_task_owns_the_pane_then_updates_in_place() {
        let mut index = TaskIndex::default();
        assert!(index.get("pane_worker").is_none());

        let pending = task(TaskState::Pending);
        assert!(index.apply_event(event::TASK_CREATED, &serde_json::json!({ "task": pending })));
        let created = task_chip(
            index.get("pane_worker").unwrap(),
            Some("orchestrator"),
            None,
        );
        assert_eq!(created.state, "Pending");
        assert_eq!(created.css_class, "task-pending");
        assert_eq!(created.lineage.as_deref(), Some("from orchestrator"));

        let mut working = task(TaskState::Working);
        working.label = "fix-parser".into();
        assert!(index.apply_event(
            event::TASK_UPDATED,
            &serde_json::json!({ "task": working, "prev_state": "pending" })
        ));
        let updated = task_chip(
            index.get("pane_worker").unwrap(),
            Some("orchestrator"),
            None,
        );
        assert_eq!(updated.label, "fix-parser");
        assert_eq!(updated.state, "Working");
        assert_eq!(updated.css_class, "task-working");

        assert!(!index.apply_event(
            event::TASK_RESULT,
            &serde_json::json!({ "task_id": "task_abc12345", "result": {} })
        ));
        assert_eq!(index.get("pane_worker").unwrap().state, TaskState::Working);

        let mut moved = task(TaskState::Working);
        moved.pane_id = Some("pane_other".into());
        assert!(index.apply_event(event::TASK_UPDATED, &serde_json::json!({ "task": moved })));
        assert!(index.get("pane_worker").is_none());
        assert_eq!(index.get("pane_other").unwrap().state, TaskState::Working);

        let mut cleared = task(TaskState::Working);
        // Same id as the moved task; clearing its pane drops the chip.
        cleared.pane_id = None;
        assert!(index.apply_event(event::TASK_UPDATED, &serde_json::json!({ "task": cleared })));
        assert!(index.get("pane_other").is_none());
    }

    #[test]
    fn a_stale_task_clearing_a_shared_pane_keeps_the_current_owners_chip() {
        let mut index = TaskIndex::default();
        let a = task(TaskState::Working);
        let mut b = task(TaskState::Working);
        b.id = "task_other999".into();
        for t in [&a, &b] {
            assert!(index.apply_event(event::TASK_UPDATED, &serde_json::json!({ "task": t })));
        }
        assert_eq!(index.get("pane_worker").unwrap().id, "task_other999");

        // A late update for A (no pane) must not drop B's chip.
        let mut a_cleared = a.clone();
        a_cleared.pane_id = None;
        assert!(index.apply_event(
            event::TASK_UPDATED,
            &serde_json::json!({ "task": a_cleared })
        ));
        assert_eq!(index.get("pane_worker").unwrap().id, "task_other999");
        assert!(index.iter().any(|t| t.id == "task_other999"));
    }

    #[test]
    fn list_seed_fills_an_empty_cache_and_a_stale_list_does_not_undo_an_update() {
        let mut index = TaskIndex::default();
        let ticket = index.seed_ticket();
        let listed = parse_task_list(&serde_json::json!([task(TaskState::Pending)])).unwrap();
        index.complete_seed(ticket, listed);
        assert_eq!(index.get("pane_worker").unwrap().state, TaskState::Pending);

        let ticket = index.seed_ticket();
        let mut working = task(TaskState::Working);
        assert!(index.apply_event(
            event::TASK_UPDATED,
            &serde_json::json!({ "task": working.clone() })
        ));
        working.state = TaskState::Pending;
        index.complete_seed(ticket, vec![working]);
        assert_eq!(index.get("pane_worker").unwrap().state, TaskState::Working);

        let ticket = index.seed_ticket();
        let mut gone = task(TaskState::Working);
        gone.pane_id = None;
        assert!(index.apply_event(event::TASK_UPDATED, &serde_json::json!({ "task": gone })));
        index.complete_seed(ticket, vec![task(TaskState::Working)]);
        assert!(
            index.get("pane_worker").is_none(),
            "a list read before the clear must not put the chip back"
        );
    }

    #[test]
    fn a_list_without_an_array_does_not_parse() {
        assert!(parse_task_list(&Value::Null).is_none());
        assert!(parse_task_list(&serde_json::json!({})).is_none());
    }
}
