//! Workspace details card (t3code's thread details): where the workspace
//! lives, what runs in it, its task and pull request, and its changes.
//! Opened from the header breadcrumb; reads git only while it is open.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use serde::Deserialize;
use serde_json::json;
use signaltty_core::{Pane, PrChecks, PrState, Workspace};

use crate::actor::IpcHandle;
use crate::task_chip::{state_word, TaskIndex};
use crate::util::tilde;

/// What the card shows, derived from the cached snapshot and tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    /// Branch, "Detached HEAD", or `None` outside a git repository.
    pub branch: Option<String>,
    pub path: String,
    /// Full path, for Copy Path and Open Folder.
    pub cwd: String,
    /// Agent display names, comma-joined; empty for plain shells.
    pub agents: String,
    pub task: Option<TaskDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDetail {
    pub label: String,
    pub state: &'static str,
    pub pr: Option<PrDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDetail {
    pub number: u64,
    pub url: String,
    pub status: &'static str,
    pub class: &'static str,
}

pub fn details(ws: &Workspace, panes: &[Pane], tasks: &TaskIndex, agents: String) -> Details {
    let branch = if ws.git.detached {
        Some("Detached HEAD".to_owned())
    } else {
        ws.git
            .branch
            .clone()
            .filter(|branch| !branch.trim().is_empty())
    };
    // A workspace can hold several task panes; the latest task speaks.
    let task = tasks
        .iter()
        .filter(|task| {
            task.pane_id
                .as_deref()
                .is_some_and(|id| panes.iter().any(|pane| pane.id == id))
        })
        .max_by_key(|task| task.updated_at)
        .map(|task| TaskDetail {
            label: task.label.clone(),
            state: state_word(task.state),
            pr: task.pr.as_ref().map(|pr| {
                let (status, class) = pr_status(pr.state, pr.checks);
                PrDetail {
                    number: pr.number,
                    url: pr.url.clone(),
                    status,
                    class,
                }
            }),
        });
    Details {
        branch,
        path: tilde(&ws.cwd),
        cwd: ws.cwd.clone(),
        agents,
        task,
    }
}

/// One word for a pull request: its end state, else its checks.
fn pr_status(state: PrState, checks: PrChecks) -> (&'static str, &'static str) {
    match (state, checks) {
        (PrState::Merged, _) => ("Merged", "pr-merged"),
        (PrState::Closed, _) => ("Closed", "pr-closed"),
        (PrState::Open, PrChecks::Failing) => ("Checks failing", "pr-failing"),
        (PrState::Open, PrChecks::Pending) => ("Checks running", "pr-pending"),
        (PrState::Open, PrChecks::Passing) => ("Checks passing", "pr-passing"),
        (PrState::Open, PrChecks::None) => ("Open", "pr-open"),
    }
}

#[derive(Deserialize)]
struct DiffTotals {
    files: Vec<serde_json::Value>,
    added: u64,
    removed: u64,
}

/// A card row: icon, text, optional trailing widget. Activatable rows are
/// flat buttons so the whole line is the target.
fn row(icon: &str, activatable: bool) -> (gtk4::Widget, gtk4::Label, gtk4::Box) {
    let image = gtk4::Image::from_icon_name(icon);
    image.add_css_class("details-icon");
    let text = gtk4::Label::new(None);
    text.set_xalign(0.0);
    text.set_hexpand(true);
    text.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    let trailing = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    trailing.set_valign(gtk4::Align::Center);
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    line.append(&image);
    line.append(&text);
    line.append(&trailing);
    let widget = if activatable {
        let button = gtk4::Button::new();
        button.set_child(Some(&line));
        button.add_css_class("flat");
        button.add_css_class("details-row");
        button.upcast()
    } else {
        line.add_css_class("details-row");
        line.upcast()
    };
    (widget, text, trailing)
}

fn section() -> gtk4::Box {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    section.add_css_class("details-section");
    section
}

/// The popover behind the breadcrumb. Its rows are built once and filled
/// on every open.
pub struct DetailsCard {
    pub popover: gtk4::Popover,
    branch: (gtk4::Widget, gtk4::Label),
    path: gtk4::Label,
    agents: (gtk4::Widget, gtk4::Label),
    task_section: gtk4::Box,
    task: gtk4::Label,
    pr: (gtk4::Button, gtk4::Label, gtk4::Label),
    stat: gtk4::Box,
    cwd: RefCell<String>,
    pr_url: RefCell<Option<String>>,
    /// Bumped per open so a slow `workspace.diff` cannot fill a newer card.
    generation: Cell<u64>,
}

impl DetailsCard {
    pub fn new() -> Rc<DetailsCard> {
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.add_css_class("details-card");

        let place = section();
        let (branch_row, branch, _) = row("signaltty-branch-symbolic", false);
        place.append(&branch_row);
        let (path_row, path, path_trailing) = row("folder-symbolic", false);
        path.set_ellipsize(gtk4::pango::EllipsizeMode::Start);
        let copy = gtk4::Button::from_icon_name("edit-copy-symbolic");
        copy.add_css_class("flat");
        copy.add_css_class("circular");
        copy.set_tooltip_text(Some("Copy Path"));
        copy.update_property(&[gtk4::accessible::Property::Label("Copy Path")]);
        path_trailing.append(&copy);
        place.append(&path_row);
        let (agents_row, agents, _) = row("utilities-terminal-symbolic", false);
        place.append(&agents_row);
        content.append(&place);

        let task_section = section();
        let (task_row, task, _) = row("system-run-symbolic", false);
        task_section.append(&task_row);
        let (pr_row, pr_text, pr_trailing) = row("signaltty-pull-request-symbolic", true);
        let pr_status = gtk4::Label::new(None);
        pr_status.add_css_class("details-pr-status");
        pr_trailing.append(&pr_status);
        task_section.append(&pr_row);
        content.append(&task_section);

        let actions = section();
        let (changes_row, changes_text, stat) = row("document-edit-symbolic", true);
        changes_text.set_text("Changes");
        stat.add_css_class("changes-stat");
        changes_row
            .downcast_ref::<gtk4::Button>()
            .unwrap()
            .set_action_name(Some("win.show-changes"));
        changes_row.set_tooltip_text(Some("Show Changes (Ctrl+Shift+D)"));
        actions.append(&changes_row);
        let (open_row, open_text, _) = row("folder-open-symbolic", true);
        open_text.set_text("Open Folder");
        actions.append(&open_row);
        content.append(&actions);

        let popover = gtk4::Popover::new();
        popover.set_child(Some(&content));
        popover.set_has_arrow(false);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.add_css_class("details-popover");

        let this = Rc::new(DetailsCard {
            popover: popover.clone(),
            branch: (branch_row, branch),
            path,
            agents: (agents_row, agents),
            task_section,
            task,
            pr: (pr_row.downcast().unwrap(), pr_text, pr_status),
            stat,
            cwd: RefCell::new(String::new()),
            pr_url: RefCell::new(None),
            generation: Cell::new(0),
        });

        let weak = Rc::downgrade(&this);
        copy.connect_clicked(move |button| {
            let Some(this) = weak.upgrade() else { return };
            button.clipboard().set_text(&this.cwd.borrow());
            // Confirm in place, then return to the copy glyph.
            button.set_icon_name("object-select-symbolic");
            let button = button.downgrade();
            gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(1200), move || {
                if let Some(button) = button.upgrade() {
                    button.set_icon_name("edit-copy-symbolic");
                }
            });
        });
        let weak = Rc::downgrade(&this);
        this.pr.0.connect_clicked(move |button| {
            let Some(this) = weak.upgrade() else { return };
            if let Some(url) = this.pr_url.borrow().clone() {
                let window = button.root().and_downcast::<gtk4::Window>();
                gtk4::UriLauncher::new(&url).launch(
                    window.as_ref(),
                    None::<&gtk4::gio::Cancellable>,
                    |_| {},
                );
            }
            this.popover.popdown();
        });
        let weak = Rc::downgrade(&this);
        open_row
            .downcast_ref::<gtk4::Button>()
            .unwrap()
            .connect_clicked(move |button| {
                let Some(this) = weak.upgrade() else { return };
                let window = button.root().and_downcast::<gtk4::Window>();
                let folder = gtk4::gio::File::for_path(&*this.cwd.borrow());
                gtk4::FileLauncher::new(Some(&folder)).launch(
                    window.as_ref(),
                    None::<&gtk4::gio::Cancellable>,
                    |_| {},
                );
                this.popover.popdown();
            });
        let weak = Rc::downgrade(&this);
        changes_row
            .downcast_ref::<gtk4::Button>()
            .unwrap()
            .connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.popover.popdown();
                }
            });
        this
    }

    /// Fill the card and start reading the workspace's change totals.
    pub fn fill(self: &Rc<Self>, details: Details, workspace: &str, actor: &IpcHandle) {
        let (branch_row, branch) = &self.branch;
        branch_row.set_visible(details.branch.is_some());
        branch.set_text(details.branch.as_deref().unwrap_or_default());
        self.path.set_text(&details.path);
        self.path.set_tooltip_text(Some(&details.cwd));
        *self.cwd.borrow_mut() = details.cwd;
        let (agents_row, agents) = &self.agents;
        agents_row.set_visible(!details.agents.is_empty());
        agents.set_text(&details.agents);

        self.task_section.set_visible(details.task.is_some());
        let (pr_row, pr_text, pr_status) = &self.pr;
        match details.task {
            Some(task) => {
                self.task
                    .set_text(&format!("Task {} · {}", task.label, task.state));
                pr_row.set_visible(task.pr.is_some());
                if let Some(pr) = &task.pr {
                    pr_text.set_text(&format!("#{}", pr.number));
                    pr_status.set_text(pr.status);
                    for class in [
                        "pr-merged",
                        "pr-closed",
                        "pr-failing",
                        "pr-pending",
                        "pr-passing",
                        "pr-open",
                    ] {
                        pr_status.remove_css_class(class);
                    }
                    pr_status.add_css_class(pr.class);
                    pr_row.set_tooltip_text(Some(&pr.url));
                    pr_row.update_property(&[gtk4::accessible::Property::Label(&format!(
                        "Pull request #{}, {}",
                        pr.number, pr.status
                    ))]);
                }
                *self.pr_url.borrow_mut() = task.pr.map(|pr| pr.url);
            }
            None => *self.pr_url.borrow_mut() = None,
        }

        self.set_stat(&[("…", "changes-tag")]);
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let this = self.clone();
        let actor = actor.clone();
        let workspace = workspace.to_owned();
        gtk4::glib::spawn_future_local(async move {
            let totals = actor
                .call(
                    signaltty_proto::method::WORKSPACE_DIFF,
                    json!({"workspace_id": workspace}),
                )
                .await
                .and_then(|value| {
                    serde_json::from_value::<DiffTotals>(value).map_err(|e| e.to_string())
                });
            if this.generation.get() != generation {
                return;
            }
            match totals {
                Ok(totals) if totals.files.is_empty() => {
                    this.set_stat(&[("No changes", "changes-tag")])
                }
                Ok(totals) => this.set_stat(&[
                    (&format!("+{}", totals.added), "file-diff-added-color"),
                    (&format!("−{}", totals.removed), "file-diff-removed-color"),
                ]),
                Err(_) => this.set_stat(&[("Unavailable", "changes-tag")]),
            }
        });
    }

    fn set_stat(&self, parts: &[(&str, &str)]) {
        while let Some(child) = self.stat.first_child() {
            self.stat.remove(&child);
        }
        for (text, class) in parts {
            let label = gtk4::Label::new(Some(text));
            label.add_css_class(class);
            label.add_css_class("numeric");
            self.stat.append(&label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use signaltty_core::PtySize;

    fn workspace(git: serde_json::Value) -> Workspace {
        let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
        Workspace {
            id: "ws".into(),
            name: "ws".into(),
            handle: "ws".into(),
            cwd: "/tmp/repo".into(),
            git: serde_json::from_value(git).unwrap(),
            tabs: vec![],
            active_tab_id: None,
            auto_resume: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn pane(id: &str) -> Pane {
        let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
        let mut pane = Pane::new(
            "ws".into(),
            "tab".into(),
            "/tmp/repo".into(),
            vec!["sh".into()],
            PtySize::default(),
            now,
        );
        pane.id = id.into();
        pane
    }

    fn task(id: &str, pane: &str, label: &str, minute: u32) -> serde_json::Value {
        let at = Utc
            .with_ymd_and_hms(2026, 10, 10, 12, minute, 0)
            .unwrap()
            .to_rfc3339();
        json!({
            "id": id, "context_id": "ctx", "pane_id": pane, "label": label,
            "contract": {"objective": "x"}, "source_repo": "/tmp/repo",
            "worktree_path": "/tmp/wt", "branch": "b", "base_ref": "HEAD",
            "base_sha": "0", "state": "working", "created_at": at, "updated_at": at
        })
    }

    #[test]
    fn branch_reads_detached_or_hides_when_blank() {
        let tasks = TaskIndex::default();
        let d = details(
            &workspace(json!({"branch": "main"})),
            &[],
            &tasks,
            String::new(),
        );
        assert_eq!(d.branch.as_deref(), Some("main"));
        let d = details(
            &workspace(json!({"detached": true})),
            &[],
            &tasks,
            String::new(),
        );
        assert_eq!(d.branch.as_deref(), Some("Detached HEAD"));
        let d = details(
            &workspace(json!({"branch": "  "})),
            &[],
            &tasks,
            String::new(),
        );
        assert_eq!(d.branch, None, "blank branch hides the row");
        assert_eq!(d.cwd, "/tmp/repo");
    }

    #[test]
    fn the_latest_task_in_the_workspace_speaks() {
        let mut tasks = TaskIndex::default();
        for value in [
            task("t_old", "p1", "old", 1),
            task("t_new", "p2", "new", 5),
            task("t_elsewhere", "p9", "elsewhere", 9),
        ] {
            assert!(tasks.apply_event(
                signaltty_proto::event::TASK_CREATED,
                &json!({"task": value})
            ));
        }
        let panes = [pane("p1"), pane("p2")];
        let d = details(&workspace(json!({})), &panes, &tasks, String::new());
        assert_eq!(d.task.unwrap().label, "new");
        let d = details(&workspace(json!({})), &[pane("p3")], &tasks, String::new());
        assert!(d.task.is_none(), "no task pane, no task section");
    }

    #[test]
    fn pull_request_says_its_end_state_before_its_checks() {
        assert_eq!(pr_status(PrState::Merged, PrChecks::Failing).0, "Merged");
        assert_eq!(pr_status(PrState::Closed, PrChecks::None).0, "Closed");
        assert_eq!(
            pr_status(PrState::Open, PrChecks::Failing).0,
            "Checks failing"
        );
        assert_eq!(
            pr_status(PrState::Open, PrChecks::Pending).0,
            "Checks running"
        );
        assert_eq!(
            pr_status(PrState::Open, PrChecks::Passing).0,
            "Checks passing"
        );
        assert_eq!(pr_status(PrState::Open, PrChecks::None).0, "Open");
    }
}
