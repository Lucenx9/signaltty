use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use signaltty_core::Workspace;

use crate::actor::IpcHandle;

pub fn rename(window: &adw::ApplicationWindow, actor: IpcHandle, workspace: &Workspace) {
    let dialog = adw::AlertDialog::new(
        Some("Rename Workspace"),
        Some("The workspace handle stays the same."),
    );
    let entry = adw::EntryRow::new();
    entry.set_title("Name");
    entry.set_text(&workspace.name);
    let group = adw::PreferencesGroup::new();
    group.add(&entry);
    dialog.set_extra_child(Some(&group));
    dialog.add_responses(&[("cancel", "Cancel"), ("rename", "Rename")]);
    dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("rename", !entry.text().trim().is_empty());
    let validate = dialog.downgrade();
    entry.connect_changed(move |entry| {
        if let Some(validate) = validate.upgrade() {
            validate.set_response_enabled("rename", !entry.text().trim().is_empty());
        }
    });
    let id = workspace.id.clone();
    let window = window.clone();
    let parent = window.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "rename" {
            return;
        }
        let name = entry.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let actor = actor.clone();
        let id = id.clone();
        let window = window.clone();
        gtk4::glib::spawn_future_local(async move {
            if let Err(error) = actor
                .call(
                    "workspace.rename",
                    json!({"workspace_id": id, "name": name}),
                )
                .await
            {
                let notice = adw::AlertDialog::new(Some("Couldn't Rename Workspace"), Some(&error));
                notice.add_response("close", "Close");
                notice.present(Some(&window));
            }
        });
    });
    dialog.present(Some(&parent));
}

fn dialog_body(title: &str) -> (adw::Dialog, gtk4::Box, gtk4::Label, gtk4::ListBox) {
    let dialog = adw::Dialog::new();
    dialog.set_title(title);
    dialog.set_content_width(680);
    dialog.set_content_height(520);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    body.append(&adw::HeaderBar::new());
    let message = gtk4::Label::new(None);
    message.set_wrap(true);
    message.set_selectable(true);
    message.set_xalign(0.0);
    message.set_margin_start(18);
    message.set_margin_end(18);
    body.append(&message);
    let rows = gtk4::ListBox::new();
    rows.set_selection_mode(gtk4::SelectionMode::None);
    rows.set_valign(gtk4::Align::Start);
    rows.add_css_class("boxed-list");
    rows.set_margin_start(18);
    rows.set_margin_end(18);
    rows.set_margin_bottom(18);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&rows));
    body.append(&scroll);
    dialog.set_child(Some(&body));
    (dialog, body, message, rows)
}

fn clear(rows: &gtk4::ListBox) {
    while let Some(child) = rows.first_child() {
        rows.remove(&child);
    }
}

#[derive(Deserialize)]
struct Worktree {
    path: String,
    branch: Option<String>,
    #[serde(default)]
    main: bool,
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    prunable: bool,
    #[serde(default)]
    bare: bool,
    workspace_id: Option<String>,
}

struct WorktreeDialog {
    dialog: adw::Dialog,
    window: adw::ApplicationWindow,
    rows: gtk4::ListBox,
    message: gtk4::Label,
    actor: IpcHandle,
    source: String,
    alive: Cell<bool>,
    loading: Cell<bool>,
    on_open: Box<dyn Fn(String)>,
}

impl WorktreeDialog {
    fn load(self: &Rc<Self>) {
        if self.loading.replace(true) {
            return;
        }
        self.message.set_text("Loading registered worktrees…");
        let this = self.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = this
                .actor
                .call("worktree.list", json!({"workspace_id": this.source}))
                .await;
            this.loading.set(false);
            if !this.alive.get() {
                return;
            }
            match result.and_then(|value| {
                serde_json::from_value::<Vec<Worktree>>(value["worktrees"].clone())
                    .map_err(|e| e.to_string())
            }) {
                Ok(worktrees) => this.render(worktrees),
                Err(error) => this.message.set_text(&error),
            }
        });
    }

    fn render(self: &Rc<Self>, worktrees: Vec<Worktree>) {
        clear(&self.rows);
        self.message.set_text("Separate checkouts for parallel work. Close their workspace before removing a clean checkout; branches are kept.");
        for worktree in worktrees {
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_title(&worktree.path);
            row.set_title_lines(0);
            let mut detail = worktree.branch.unwrap_or_else(|| "Detached HEAD".into());
            if worktree.main {
                detail.push_str(" · Main checkout");
            }
            if worktree.locked {
                detail.push_str(" · Locked");
            }
            if worktree.prunable {
                detail.push_str(" · Missing checkout");
            }
            if worktree.workspace_id.is_some() {
                detail.push_str(" · Workspace open");
            }
            row.set_subtitle(&detail);
            row.set_subtitle_lines(0);
            let actions = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            actions.set_valign(gtk4::Align::Center);
            let open = gtk4::Button::with_label("Open");
            open.set_sensitive(!worktree.prunable && !worktree.bare);
            let this = Rc::downgrade(self);
            let path = worktree.path.clone();
            open.connect_clicked(move |_| {
                if let Some(this) = this.upgrade() {
                    this.open(
                        "worktree.open",
                        json!({"workspace_id": this.source, "path": path}),
                    );
                }
            });
            actions.append(&open);
            let remove = gtk4::Button::with_label("Remove…");
            remove.add_css_class("flat");
            remove.set_sensitive(
                !worktree.main
                    && !worktree.locked
                    && !worktree.prunable
                    && !worktree.bare
                    && worktree.workspace_id.is_none(),
            );
            let this = Rc::downgrade(self);
            let path = worktree.path;
            remove.connect_clicked(move |_| {
                if let Some(this) = this.upgrade() {
                    this.confirm_remove(&path);
                }
            });
            actions.append(&remove);
            row.add_suffix(&actions);
            self.rows.append(&row);
        }
    }

    fn open(self: &Rc<Self>, method: &'static str, params: Value) {
        if self.loading.replace(true) {
            return;
        }
        self.message.set_text("Opening workspace…");
        let this = self.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = this.actor.call(method, params).await;
            this.loading.set(false);
            if !this.alive.get() {
                return;
            }
            match result {
                Ok(value) => {
                    if let Some(id) = value["workspace"]["id"].as_str() {
                        this.dialog.close();
                        (this.on_open)(id.into());
                    } else {
                        this.message
                            .set_text("The server did not return a workspace.");
                    }
                }
                Err(error) => this.message.set_text(&error),
            }
        });
    }

    fn create(self: &Rc<Self>) {
        let dialog = adw::AlertDialog::new(
            Some("Create Worktree"),
            Some("Create a new branch in a separate checkout."),
        );
        let group = adw::PreferencesGroup::new();
        let path = adw::EntryRow::new();
        path.set_title("Absolute checkout path");
        let branch = adw::EntryRow::new();
        branch.set_title("New branch");
        group.add(&path);
        group.add(&branch);
        dialog.set_extra_child(Some(&group));
        dialog.add_responses(&[("cancel", "Cancel"), ("create", "Create")]);
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("create"));
        dialog.set_close_response("cancel");
        dialog.set_response_enabled("create", false);
        let validate = Rc::new({
            let dialog = dialog.downgrade();
            let path = path.downgrade();
            let branch = branch.downgrade();
            move || {
                if let (Some(dialog), Some(path), Some(branch)) =
                    (dialog.upgrade(), path.upgrade(), branch.upgrade())
                {
                    dialog.set_response_enabled(
                        "create",
                        path.text().starts_with('/') && !branch.text().trim().is_empty(),
                    );
                }
            }
        });
        let check = validate.clone();
        path.connect_changed(move |_| check());
        branch.connect_changed(move |_| validate());
        let this = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "create" && this.alive.get() {
                this.open("worktree.create", json!({"workspace_id": this.source, "path": path.text().as_str(), "branch": branch.text().trim()}));
            }
        });
        dialog.present(Some(&self.window));
    }

    fn confirm_remove(self: &Rc<Self>, path: &str) {
        let dialog = adw::AlertDialog::new(Some("Remove Worktree?"), Some(&format!("Delete the clean checkout at {path}? Its branch will be kept. Dirty or active checkouts cannot be removed.")));
        dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
        dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let this = self.clone();
        let path = path.to_string();
        dialog.connect_response(None, move |_, response| {
            if response != "remove" || !this.alive.get() || this.loading.replace(true) {
                return;
            }
            let this = this.clone();
            let path = path.clone();
            gtk4::glib::spawn_future_local(async move {
                let result = this
                    .actor
                    .call(
                        "worktree.remove",
                        json!({"workspace_id": this.source, "path": path}),
                    )
                    .await;
                this.loading.set(false);
                if !this.alive.get() {
                    return;
                }
                match result {
                    Ok(_) => this.load(),
                    Err(error) => this.message.set_text(&error),
                }
            });
        });
        dialog.present(Some(&self.window));
    }
}

pub fn worktrees(
    window: &adw::ApplicationWindow,
    actor: IpcHandle,
    source: &str,
    on_open: impl Fn(String) + 'static,
) {
    let (dialog, body, message, rows) = dialog_body("Worktrees");
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    controls.set_halign(gtk4::Align::End);
    controls.set_margin_end(18);
    let create = gtk4::Button::with_label("Create Worktree…");
    create.add_css_class("suggested-action");
    let refresh = gtk4::Button::with_label("Refresh");
    controls.append(&refresh);
    controls.append(&create);
    body.insert_child_after(&controls, Some(&message));
    let this = Rc::new(WorktreeDialog {
        dialog: dialog.clone(),
        window: window.clone(),
        rows,
        message,
        actor,
        source: source.into(),
        alive: Cell::new(true),
        loading: Cell::new(false),
        on_open: Box::new(on_open),
    });
    let keepalive = RefCell::new(Some(this.clone()));
    dialog.connect_closed(move |_| {
        if let Some(this) = keepalive.borrow_mut().take() {
            this.alive.set(false);
        }
    });
    let creator = Rc::downgrade(&this);
    create.connect_clicked(move |_| {
        if let Some(this) = creator.upgrade() {
            this.create();
        }
    });
    let loader = Rc::downgrade(&this);
    refresh.connect_clicked(move |_| {
        if let Some(this) = loader.upgrade() {
            this.load();
        }
    });
    dialog.present(Some(window));
    dialog.set_focus(Some(&create));
    this.load();
}
