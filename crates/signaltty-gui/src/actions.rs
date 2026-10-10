//! Single registry for window actions: each action is defined once
//! (name, menu label + section, accelerator) and this module builds
//! the menu model, installs the gio actions and sets the accelerators
//! from that table. A typo used to break one of the three silently
//! (dead menu item, dead shortcut); now the table is the only place
//! names are spelled, and the tests below pin the consistency.
//!
//! Handlers arrive as callbacks (see [`ActionHandlers`]), like
//! [`PaneCallbacks`](crate::terminal::PaneCallbacks): App owns the
//! behavior, the registry owns the wiring. Header/sidebar buttons
//! still reference actions by `"win.…"` name; those names live here.

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use signaltty_core::{DispositionOutcome, Task, TaskState};

/// Which behavior an action triggers. `ToggleSidebar` is a property
/// action bound to the split view; the rest take callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerKind {
    Palette,
    RenameWorkspace,
    SearchTerminal,
    ZoomPane,
    Worktrees,
    ShowChanges,
    ShowBoard,
    NewWorkspace,
    CloseWorkspace,
    NewTab,
    SplitRight,
    SplitDown,
    ClosePane,
    NextAttention,
    Preferences,
    About,
    ToggleSidebar,
}

pub struct ActionDef {
    /// Short name (`"new-workspace"`); builders add the `"win."` prefix.
    pub name: &'static str,
    pub handler: HandlerKind,
    /// Menu label; `None` keeps the action out of the menu.
    pub label: Option<&'static str>,
    /// Menu section; entries render grouped and ordered by it.
    pub section: u8,
    /// Accelerator (Ctrl+Shift: plain Ctrl chords belong to the
    /// programs running in the terminals).
    pub accel: Option<&'static str>,
}

pub const ACTIONS: &[ActionDef] = &[
    ActionDef {
        name: "show-changes",
        handler: HandlerKind::ShowChanges,
        label: Some("Show Changes"),
        section: 0,
        accel: Some("<Control><Shift>d"),
    },
    ActionDef {
        name: "show-board",
        handler: HandlerKind::ShowBoard,
        label: Some("Show Task Board"),
        section: 0,
        accel: Some("<Control><Shift>b"),
    },
    ActionDef {
        name: "worktrees",
        handler: HandlerKind::Worktrees,
        label: Some("Worktrees"),
        section: 0,
        accel: None,
    },
    ActionDef {
        name: "zoom-pane",
        handler: HandlerKind::ZoomPane,
        label: Some("Zoom Pane"),
        section: 1,
        accel: Some("<Control><Shift>z"),
    },
    ActionDef {
        name: "search-terminal",
        handler: HandlerKind::SearchTerminal,
        label: Some("Find in Terminal"),
        section: 1,
        accel: Some("<Control><Shift>f"),
    },
    ActionDef {
        name: "rename-workspace",
        handler: HandlerKind::RenameWorkspace,
        label: Some("Rename Workspace"),
        section: 0,
        accel: Some("<Control><Shift>r"),
    },
    ActionDef {
        name: "command-palette",
        handler: HandlerKind::Palette,
        label: Some("Commands and Workspaces"),
        section: 0,
        accel: Some("<Control><Shift>p"),
    },
    ActionDef {
        name: "new-workspace",
        handler: HandlerKind::NewWorkspace,
        label: Some("New Workspace"),
        section: 0,
        accel: Some("<Control><Shift>n"),
    },
    ActionDef {
        name: "close-workspace",
        handler: HandlerKind::CloseWorkspace,
        label: Some("Close Workspace"),
        section: 0,
        accel: None,
    },
    ActionDef {
        name: "new-tab",
        handler: HandlerKind::NewTab,
        label: Some("New Tab"),
        section: 0,
        accel: Some("<Control><Shift>t"),
    },
    ActionDef {
        name: "split-right",
        handler: HandlerKind::SplitRight,
        label: Some("Split Right"),
        section: 1,
        accel: Some("<Control><Shift>e"),
    },
    ActionDef {
        name: "split-down",
        handler: HandlerKind::SplitDown,
        label: Some("Split Down"),
        section: 1,
        accel: Some("<Control><Shift>o"),
    },
    ActionDef {
        name: "close-pane",
        handler: HandlerKind::ClosePane,
        label: Some("Close Pane"),
        section: 1,
        accel: Some("<Control><Shift>w"),
    },
    ActionDef {
        name: "next-attention",
        handler: HandlerKind::NextAttention,
        label: Some("Next Pane Needing Attention"),
        section: 2,
        accel: Some("<Control><Shift>j"),
    },
    ActionDef {
        name: "preferences",
        handler: HandlerKind::Preferences,
        label: Some("Preferences"),
        section: 3,
        accel: Some("<Control>comma"),
    },
    ActionDef {
        name: "about",
        handler: HandlerKind::About,
        label: Some("About signaltty"),
        section: 3,
        accel: None,
    },
    ActionDef {
        name: "toggle-sidebar",
        handler: HandlerKind::ToggleSidebar,
        label: None,
        section: 0,
        accel: Some("F9"),
    },
];

/// App behavior behind the actions, one callback each.
pub struct ActionHandlers {
    pub show_changes: Box<dyn Fn()>,
    pub show_board: Box<dyn Fn()>,
    pub worktrees: Box<dyn Fn()>,
    pub zoom_pane: Box<dyn Fn()>,
    pub search_terminal: Box<dyn Fn()>,
    pub rename_workspace: Box<dyn Fn()>,
    pub palette: Box<dyn Fn()>,
    pub new_workspace: Box<dyn Fn()>,
    pub close_workspace: Box<dyn Fn()>,
    pub new_tab: Box<dyn Fn()>,
    pub split_right: Box<dyn Fn()>,
    pub split_down: Box<dyn Fn()>,
    pub close_pane: Box<dyn Fn()>,
    pub next_attention: Box<dyn Fn()>,
    pub preferences: Box<dyn Fn()>,
    pub about: Box<dyn Fn()>,
}

impl ActionHandlers {
    fn get(&self, kind: HandlerKind) -> &dyn Fn() {
        match kind {
            HandlerKind::ShowChanges => &self.show_changes,
            HandlerKind::ShowBoard => &self.show_board,
            HandlerKind::Worktrees => &self.worktrees,
            HandlerKind::ZoomPane => &self.zoom_pane,
            HandlerKind::SearchTerminal => &self.search_terminal,
            HandlerKind::RenameWorkspace => &self.rename_workspace,
            HandlerKind::Palette => &self.palette,
            HandlerKind::NewWorkspace => &self.new_workspace,
            HandlerKind::CloseWorkspace => &self.close_workspace,
            HandlerKind::NewTab => &self.new_tab,
            HandlerKind::SplitRight => &self.split_right,
            HandlerKind::SplitDown => &self.split_down,
            HandlerKind::ClosePane => &self.close_pane,
            HandlerKind::NextAttention => &self.next_attention,
            HandlerKind::Preferences => &self.preferences,
            HandlerKind::About => &self.about,
            HandlerKind::ToggleSidebar => unreachable!("property action has no callback"),
        }
    }
}

/// Menu layout, pure data: sections of `(label, "win."-prefixed action)`.
pub fn menu_sections() -> Vec<Vec<(&'static str, String)>> {
    let mut sections: Vec<Vec<(&'static str, String)>> = Vec::new();
    for def in ACTIONS {
        let Some(label) = def.label else {
            continue;
        };
        while sections.len() <= def.section as usize {
            sections.push(Vec::new());
        }
        sections[def.section as usize].push((label, format!("win.{}", def.name)));
    }
    sections.retain(|s| !s.is_empty());
    sections
}

/// Primary menu, rendered from [`menu_sections`].
pub fn primary_menu() -> gio::Menu {
    let menu = gio::Menu::new();
    for items in menu_sections() {
        let section = gio::Menu::new();
        for (label, action) in &items {
            section.append(Some(label), Some(action));
        }
        menu.append_section(None, &section);
    }
    menu
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskMenuFacts {
    pub task_id: String,
    pub state: TaskState,
    pub outcome: DispositionOutcome,
    pub has_pr: bool,
    pub pr_url: Option<String>,
    pub target: Option<String>,
}

impl TaskMenuFacts {
    pub fn from_task(task: &Task) -> Self {
        Self {
            task_id: task.id.clone(),
            state: task.state,
            outcome: task.disposition.outcome,
            has_pr: task.pr.is_some(),
            pr_url: task
                .pr
                .as_ref()
                .map(|p| p.url.clone())
                .filter(|u| !u.is_empty()),
            target: task
                .disposition
                .target_ref
                .clone()
                .or_else(|| task.target_branch.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarRowKind {
    Root {
        ws_id: String,
        finished_children_count: usize,
    },
    Child(TaskMenuFacts),
}

/// One context-menu entry: label, `win.`-prefixed action, string target.
pub type MenuEntry = (String, String, Option<String>);

/// Entry for an action from the table, with its table label.
fn table_entry(name: &str) -> MenuEntry {
    let def = ACTIONS
        .iter()
        .find(|d| d.name == name)
        .expect("known sidebar action");
    let label = def.label.expect("sidebar action has a menu label");
    (label.to_string(), format!("win.{name}"), None)
}

fn task_entry(label: impl Into<String>, action: &str, target: &str) -> MenuEntry {
    (
        label.into(),
        format!("win.{action}"),
        Some(target.to_string()),
    )
}

/// Menu sections for a sidebar row. Task items that don't apply to the
/// task's state are omitted rather than shown disabled.
pub fn sidebar_menu_sections(kind: &SidebarRowKind) -> Vec<Vec<MenuEntry>> {
    let close = vec![table_entry("close-workspace")];
    match kind {
        SidebarRowKind::Root {
            ws_id,
            finished_children_count: n,
        } => {
            let mut sections = vec![["rename-workspace", "show-changes", "worktrees", "new-tab"]
                .map(table_entry)
                .to_vec()];
            if *n > 0 {
                let label = if *n == 1 {
                    "Clear 1 Finished Task".to_string()
                } else {
                    format!("Clear {n} Finished Tasks")
                };
                sections.push(vec![task_entry(label, "clear-finished-tasks", ws_id)]);
            }
            sections.push(close);
            sections
        }
        SidebarRowKind::Child(task) => {
            let id = task.task_id.as_str();
            let open = task.outcome == DispositionOutcome::None;
            let completed = task.state == TaskState::Completed;
            let mut items = vec![table_entry("show-board")];
            if task.pr_url.is_some() {
                items.push(task_entry("Open Pull Request", "task-open-pr", id));
            }
            if completed && open && !task.has_pr {
                items.push(task_entry("Create Pull Request", "task-create-pr", id));
            }
            if completed && open {
                let label = match &task.target {
                    Some(target) => format!("Merge into {target}"),
                    None => "Merge".to_string(),
                };
                items.push(task_entry(label, "task-merge", id));
            }
            if matches!(
                task.state,
                TaskState::Pending | TaskState::Working | TaskState::InputRequired
            ) {
                items.push(task_entry("Cancel Task", "task-cancel", id));
            }
            if open && task.state != TaskState::Canceled {
                items.push(task_entry("Discard Task", "task-discard", id));
            }
            vec![
                items,
                ["rename-workspace", "show-changes", "new-tab"]
                    .map(table_entry)
                    .to_vec(),
                close,
            ]
        }
    }
}

/// Flattened entries, for tests.
#[cfg(test)]
pub fn sidebar_menu_items(kind: &SidebarRowKind) -> Vec<MenuEntry> {
    sidebar_menu_sections(kind).into_iter().flatten().collect()
}

/// Builds a `gio::Menu` model for a sidebar row context menu.
pub fn build_sidebar_menu(kind: &SidebarRowKind) -> gio::Menu {
    let menu = gio::Menu::new();
    for entries in sidebar_menu_sections(kind) {
        let section = gio::Menu::new();
        for (label, action, target) in entries {
            let item = gio::MenuItem::new(Some(&label), None);
            item.set_action_and_target_value(
                Some(&action),
                target.map(|t| t.to_variant()).as_ref(),
            );
            section.append_item(&item);
        }
        menu.append_section(None, &section);
    }
    menu
}

pub struct TaskActionHandlers {
    pub task_open_pr: Box<dyn Fn(String)>,
    pub task_create_pr: Box<dyn Fn(String)>,
    pub task_merge: Box<dyn Fn(String)>,
    pub task_cancel: Box<dyn Fn(String)>,
    pub task_discard: Box<dyn Fn(String)>,
    pub clear_finished_tasks: Box<dyn Fn(String)>,
}

/// Install every gio action and accelerator from the table and task handlers.
pub fn install(
    window: &adw::ApplicationWindow,
    application: &adw::Application,
    split_view: &adw::OverlaySplitView,
    handlers: ActionHandlers,
    task_handlers: TaskActionHandlers,
) {
    let handlers = std::rc::Rc::new(handlers);
    for def in ACTIONS {
        match def.handler {
            HandlerKind::ToggleSidebar => {
                window.add_action(&gio::PropertyAction::new(
                    def.name,
                    split_view,
                    "show-sidebar",
                ));
            }
            kind => {
                let action = gio::SimpleAction::new(def.name, None);
                // The match on HandlerKind (not on the name string) is
                // exhaustive: a table entry without behavior is a
                // compile error, not a dead menu item.
                let handlers = std::rc::Rc::clone(&handlers);
                action.connect_activate(move |_, _| handlers.get(kind)());
                window.add_action(&action);
            }
        }
        if let Some(accel) = def.accel {
            application.set_accels_for_action(&format!("win.{}", def.name), &[accel]);
        }
    }

    let add_task_action = |name: &str, cb: Box<dyn Fn(String)>| {
        let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
        action.connect_activate(move |_, param| {
            if let Some(arg) = param.and_then(|p| p.get::<String>()) {
                cb(arg);
            }
        });
        window.add_action(&action);
    };

    add_task_action("task-open-pr", task_handlers.task_open_pr);
    add_task_action("task-create-pr", task_handlers.task_create_pr);
    add_task_action("task-merge", task_handlers.task_merge);
    add_task_action("task-cancel", task_handlers.task_cancel);
    add_task_action("task-discard", task_handlers.task_discard);
    add_task_action("clear-finished-tasks", task_handlers.clear_finished_tasks);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_names_are_unique() {
        let mut names: Vec<&str> = ACTIONS.iter().map(|d| d.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), ACTIONS.len());
    }

    #[test]
    fn menu_covers_exactly_the_labelled_actions() {
        let sections = menu_sections();
        let in_menu: Vec<String> = sections.iter().flatten().map(|(_, a)| a.clone()).collect();
        let labelled: Vec<String> = ACTIONS
            .iter()
            .filter(|d| d.label.is_some())
            .map(|d| format!("win.{}", d.name))
            .collect();
        assert_eq!(in_menu.len(), labelled.len());
        for action in &labelled {
            assert!(in_menu.contains(action), "{action} missing from menu");
        }
        // Sections stay dense and ordered; labels are non-empty.
        assert_eq!(sections.len(), 4);
        for items in &sections {
            assert!(!items.is_empty());
            for (label, _) in items {
                assert!(!label.is_empty());
            }
        }
    }

    #[test]
    fn every_action_is_reachable() {
        // No dead entries: each action has a menu label, a shortcut,
        // or is the sidebar toggle (header button + F9).
        for def in ACTIONS {
            let reachable = def.label.is_some()
                || def.accel.is_some()
                || def.handler == HandlerKind::ToggleSidebar;
            assert!(reachable, "{} is unreachable", def.name);
        }
    }

    #[test]
    fn accels_are_unique() {
        let mut accels: Vec<&str> = ACTIONS.iter().filter_map(|d| d.accel).collect();
        accels.sort_unstable();
        let len = accels.len();
        accels.dedup();
        assert_eq!(accels.len(), len);
    }

    #[test]
    fn rendered_menu_matches_table() {
        // gio::Menu needs no display: assert the built model carries
        // the win.-prefixed names, section by section.
        let menu = primary_menu();
        let model = menu.upcast_ref::<gio::MenuModel>();
        let sections = menu_sections();
        assert_eq!(model.n_items() as usize, sections.len());
        for (i, items) in sections.iter().enumerate() {
            let link = model
                .item_link(i as i32, gio::MENU_LINK_SECTION)
                .expect("section link");
            assert_eq!(link.n_items() as usize, items.len());
            for (j, (label, action)) in items.iter().enumerate() {
                let mut got_label = None;
                let mut got_action = None;
                let attrs = link.iterate_item_attributes(j as i32);
                while let Some((name, value)) = attrs.next() {
                    if name == "label" {
                        got_label = value.get::<String>();
                    } else if name == "action" {
                        got_action = value.get::<String>();
                    }
                }
                assert_eq!(got_label.as_deref(), Some(*label));
                assert_eq!(got_action.as_deref(), Some(action.as_str()));
            }
        }
    }

    #[test]
    fn sidebar_menu_matches_spec() {
        let menu = build_sidebar_menu(&SidebarRowKind::Root {
            ws_id: "ws".into(),
            finished_children_count: 0,
        });
        let model = menu.upcast_ref::<gio::MenuModel>();
        assert_eq!(model.n_items(), 2, "two sections");

        let sec0 = model.item_link(0, gio::MENU_LINK_SECTION).unwrap();
        assert_eq!(sec0.n_items(), 4);
        let expected_sec0 = [
            ("Rename Workspace", "win.rename-workspace"),
            ("Show Changes", "win.show-changes"),
            ("Worktrees", "win.worktrees"),
            ("New Tab", "win.new-tab"),
        ];
        for (i, (exp_label, exp_action)) in expected_sec0.iter().enumerate() {
            let mut label = None;
            let mut action = None;
            let attrs = sec0.iterate_item_attributes(i as i32);
            while let Some((name, val)) = attrs.next() {
                if name == "label" {
                    label = val.get::<String>();
                } else if name == "action" {
                    action = val.get::<String>();
                }
            }
            assert_eq!(label.as_deref(), Some(*exp_label));
            assert_eq!(action.as_deref(), Some(*exp_action));
        }

        let sec1 = model.item_link(1, gio::MENU_LINK_SECTION).unwrap();
        assert_eq!(sec1.n_items(), 1);
        let mut label = None;
        let mut action = None;
        let attrs = sec1.iterate_item_attributes(0);
        while let Some((name, val)) = attrs.next() {
            if name == "label" {
                label = val.get::<String>();
            } else if name == "action" {
                action = val.get::<String>();
            }
        }
        assert_eq!(label.as_deref(), Some("Close Workspace"));
        assert_eq!(action.as_deref(), Some("win.close-workspace"));
    }

    #[test]
    fn sidebar_menu_pure_builder_root_without_finished() {
        let items = sidebar_menu_items(&SidebarRowKind::Root {
            ws_id: "ws-root".into(),
            finished_children_count: 0,
        });
        assert_eq!(
            items,
            vec![
                (
                    "Rename Workspace".into(),
                    "win.rename-workspace".into(),
                    None
                ),
                ("Show Changes".into(), "win.show-changes".into(), None),
                ("Worktrees".into(), "win.worktrees".into(), None),
                ("New Tab".into(), "win.new-tab".into(), None),
                ("Close Workspace".into(), "win.close-workspace".into(), None),
            ]
        );
    }

    #[test]
    fn sidebar_menu_pure_builder_root_with_finished() {
        let items = sidebar_menu_items(&SidebarRowKind::Root {
            ws_id: "ws-root".into(),
            finished_children_count: 2,
        });
        assert_eq!(
            items,
            vec![
                (
                    "Rename Workspace".into(),
                    "win.rename-workspace".into(),
                    None
                ),
                ("Show Changes".into(), "win.show-changes".into(), None),
                ("Worktrees".into(), "win.worktrees".into(), None),
                ("New Tab".into(), "win.new-tab".into(), None),
                (
                    "Clear 2 Finished Tasks".into(),
                    "win.clear-finished-tasks".into(),
                    Some("ws-root".into())
                ),
                ("Close Workspace".into(), "win.close-workspace".into(), None),
            ]
        );

        let items_single = sidebar_menu_items(&SidebarRowKind::Root {
            ws_id: "ws-root".into(),
            finished_children_count: 1,
        });
        assert_eq!(
            items_single[4],
            (
                "Clear 1 Finished Task".into(),
                "win.clear-finished-tasks".into(),
                Some("ws-root".into())
            )
        );
    }

    #[test]
    fn sidebar_menu_pure_builder_child_completed_no_pr() {
        let facts = TaskMenuFacts {
            task_id: "task-1".into(),
            state: TaskState::Completed,
            outcome: DispositionOutcome::None,
            has_pr: false,
            pr_url: None,
            target: Some("main".into()),
        };
        let items = sidebar_menu_items(&SidebarRowKind::Child(facts));
        assert_eq!(
            items,
            vec![
                ("Show Task Board".into(), "win.show-board".into(), None),
                (
                    "Create Pull Request".into(),
                    "win.task-create-pr".into(),
                    Some("task-1".into())
                ),
                (
                    "Merge into main".into(),
                    "win.task-merge".into(),
                    Some("task-1".into())
                ),
                (
                    "Discard Task".into(),
                    "win.task-discard".into(),
                    Some("task-1".into())
                ),
                (
                    "Rename Workspace".into(),
                    "win.rename-workspace".into(),
                    None
                ),
                ("Show Changes".into(), "win.show-changes".into(), None),
                ("New Tab".into(), "win.new-tab".into(), None),
                ("Close Workspace".into(), "win.close-workspace".into(), None),
            ]
        );
    }

    #[test]
    fn sidebar_menu_pure_builder_child_completed_with_pr() {
        let facts = TaskMenuFacts {
            task_id: "task-2".into(),
            state: TaskState::Completed,
            outcome: DispositionOutcome::None,
            has_pr: true,
            pr_url: Some("https://github.com/org/repo/pull/42".into()),
            target: None,
        };
        let items = sidebar_menu_items(&SidebarRowKind::Child(facts));
        // Omission rule: Create PR omitted because has_pr is true.
        // Target is None so label is "Merge" not "Merge into".
        // Cancel omitted because state is Completed.
        assert_eq!(
            items,
            vec![
                ("Show Task Board".into(), "win.show-board".into(), None),
                (
                    "Open Pull Request".into(),
                    "win.task-open-pr".into(),
                    Some("task-2".into())
                ),
                (
                    "Merge".into(),
                    "win.task-merge".into(),
                    Some("task-2".into())
                ),
                (
                    "Discard Task".into(),
                    "win.task-discard".into(),
                    Some("task-2".into())
                ),
                (
                    "Rename Workspace".into(),
                    "win.rename-workspace".into(),
                    None
                ),
                ("Show Changes".into(), "win.show-changes".into(), None),
                ("New Tab".into(), "win.new-tab".into(), None),
                ("Close Workspace".into(), "win.close-workspace".into(), None),
            ]
        );
    }

    #[test]
    fn sidebar_menu_pure_builder_child_active_states() {
        for state in [
            TaskState::Pending,
            TaskState::Working,
            TaskState::InputRequired,
        ] {
            let facts = TaskMenuFacts {
                task_id: "task-active".into(),
                state,
                outcome: DispositionOutcome::None,
                has_pr: false,
                pr_url: None,
                target: Some("feat".into()),
            };
            let items = sidebar_menu_items(&SidebarRowKind::Child(facts));
            // Omission rule: Create PR and Merge omitted because state is not Completed.
            // Cancel Task and Discard Task present.
            assert_eq!(
                items,
                vec![
                    ("Show Task Board".into(), "win.show-board".into(), None),
                    (
                        "Cancel Task".into(),
                        "win.task-cancel".into(),
                        Some("task-active".into())
                    ),
                    (
                        "Discard Task".into(),
                        "win.task-discard".into(),
                        Some("task-active".into())
                    ),
                    (
                        "Rename Workspace".into(),
                        "win.rename-workspace".into(),
                        None
                    ),
                    ("Show Changes".into(), "win.show-changes".into(), None),
                    ("New Tab".into(), "win.new-tab".into(), None),
                    ("Close Workspace".into(), "win.close-workspace".into(), None),
                ]
            );
        }
    }

    #[test]
    fn sidebar_menu_pure_builder_child_merged_or_discarded() {
        for outcome in [DispositionOutcome::Merged, DispositionOutcome::Discarded] {
            let facts = TaskMenuFacts {
                task_id: "task-done".into(),
                state: TaskState::Completed,
                outcome,
                has_pr: true,
                pr_url: Some("https://pr".into()),
                target: Some("main".into()),
            };
            let items = sidebar_menu_items(&SidebarRowKind::Child(facts));
            // Omission rule: Merge, Create PR, Cancel Task, Discard Task all omitted because outcome is not None.
            // Open PR remains because pr_url is present.
            assert_eq!(
                items,
                vec![
                    ("Show Task Board".into(), "win.show-board".into(), None),
                    (
                        "Open Pull Request".into(),
                        "win.task-open-pr".into(),
                        Some("task-done".into())
                    ),
                    (
                        "Rename Workspace".into(),
                        "win.rename-workspace".into(),
                        None
                    ),
                    ("Show Changes".into(), "win.show-changes".into(), None),
                    ("New Tab".into(), "win.new-tab".into(), None),
                    ("Close Workspace".into(), "win.close-workspace".into(), None),
                ]
            );
        }
    }

    #[test]
    fn sidebar_menu_pure_builder_child_canceled() {
        let facts = TaskMenuFacts {
            task_id: "task-canceled".into(),
            state: TaskState::Canceled,
            outcome: DispositionOutcome::None,
            has_pr: false,
            pr_url: None,
            target: Some("main".into()),
        };
        let items = sidebar_menu_items(&SidebarRowKind::Child(facts));
        // Omission rule: Cancel Task omitted because already canceled.
        // Discard Task omitted because state == Canceled.
        // Merge & Create PR omitted because state != Completed.
        assert_eq!(
            items,
            vec![
                ("Show Task Board".into(), "win.show-board".into(), None),
                (
                    "Rename Workspace".into(),
                    "win.rename-workspace".into(),
                    None
                ),
                ("Show Changes".into(), "win.show-changes".into(), None),
                ("New Tab".into(), "win.new-tab".into(), None),
                ("Close Workspace".into(), "win.close-workspace".into(), None),
            ]
        );
    }
}
