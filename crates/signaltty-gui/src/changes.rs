use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use serde::Deserialize;
use serde_json::json;
use signaltty_core::diff::{DiffContent, DiffLineKind, FileDiff};

use crate::actor::IpcHandle;

#[derive(Deserialize)]
struct SummaryFile {
    path: String,
    added: u64,
    removed: u64,
    binary: bool,
    untracked: bool,
}

#[derive(Deserialize)]
struct Summary {
    files: Vec<SummaryFile>,
    added: u64,
    removed: u64,
}

struct ChangesDialog {
    actor: IpcHandle,
    workspace: String,
    navigation: adw::NavigationView,
    reader_page: adw::NavigationPage,
    rows: gtk4::ListBox,
    summary: gtk4::Label,
    path: gtk4::Label,
    status: gtk4::Label,
    reader: gtk4::TextView,
    added_color: gtk4::Label,
    removed_color: gtk4::Label,
    row_map: RefCell<BTreeMap<String, adw::ActionRow>>,
    selected: RefCell<Option<String>>,
    alive: Cell<bool>,
    summary_generation: Cell<u64>,
    detail_generation: Cell<u64>,
    style_handlers: RefCell<Vec<gtk4::glib::SignalHandlerId>>,
}

fn label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_wrap(true);
    label.set_selectable(true);
    label.set_xalign(0.0);
    label.set_margin_start(18);
    label.set_margin_end(18);
    label
}

fn refresh_button(header: &adw::HeaderBar) -> gtk4::Button {
    let button = gtk4::Button::with_label("Refresh");
    button.set_tooltip_text(Some("Refresh working tree changes"));
    header.pack_end(&button);
    button
}

pub fn present(window: &adw::ApplicationWindow, actor: IpcHandle, workspace: &str) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Working Tree Changes");
    dialog.set_content_width(680);
    dialog.set_content_height(520);
    let navigation = adw::NavigationView::new();
    navigation.set_animate_transitions(false);
    navigation.set_pop_on_escape(true);

    let list_body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    let list_header = adw::HeaderBar::new();
    let list_refresh = refresh_button(&list_header);
    list_body.append(&list_header);
    let summary = label();
    list_body.append(&summary);
    let rows = gtk4::ListBox::new();
    rows.set_selection_mode(gtk4::SelectionMode::Single);
    rows.set_sort_func(|left, right| {
        let left = left.downcast_ref::<adw::ActionRow>().unwrap().title();
        let right = right.downcast_ref::<adw::ActionRow>().unwrap().title();
        left.cmp(&right).into()
    });
    rows.set_valign(gtk4::Align::Start);
    rows.add_css_class("boxed-list");
    rows.set_margin_start(18);
    rows.set_margin_end(18);
    rows.set_margin_bottom(18);
    let list_scroll = gtk4::ScrolledWindow::new();
    list_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    list_scroll.set_vexpand(true);
    list_scroll.set_child(Some(&rows));
    list_body.append(&list_scroll);
    let list_page = adw::NavigationPage::new(&list_body, "Working Tree Changes");
    navigation.add(&list_page);

    let reader_body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    let reader_header = adw::HeaderBar::new();
    reader_header.set_show_back_button(false);
    let back = gtk4::Button::with_label("Back");
    back.set_tooltip_text(Some("Return to changed files (Alt+Left)"));
    reader_header.pack_start(&back);
    let reader_refresh = refresh_button(&reader_header);
    reader_body.append(&reader_header);
    let path = label();
    path.add_css_class("heading");
    reader_body.append(&path);
    let scope = label();
    scope.set_text("Changes against HEAD");
    scope.add_css_class("dim-label");
    reader_body.append(&scope);
    let status = label();
    reader_body.append(&status);
    let added_color = gtk4::Label::new(None);
    added_color.add_css_class("file-diff-added-color");
    added_color.set_visible(false);
    reader_body.append(&added_color);
    let removed_color = gtk4::Label::new(None);
    removed_color.add_css_class("file-diff-removed-color");
    removed_color.set_visible(false);
    reader_body.append(&removed_color);
    let reader = gtk4::TextView::new();
    reader.set_editable(false);
    reader.set_monospace(true);
    reader.set_wrap_mode(gtk4::WrapMode::None);
    reader.set_left_margin(16);
    reader.set_right_margin(16);
    reader.set_top_margin(14);
    reader.set_bottom_margin(14);
    reader.set_pixels_above_lines(1);
    reader.set_pixels_below_lines(1);
    reader.update_property(&[gtk4::accessible::Property::Label(
        "File diff with old and new line numbers",
    )]);
    let buffer = reader.buffer();
    buffer.create_tag(Some("added"), &[]);
    buffer.create_tag(Some("removed"), &[]);
    buffer.create_tag(Some("heading"), &[("weight", &700i32)]);
    let reader_scroll = gtk4::ScrolledWindow::new();
    reader_scroll.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    reader_scroll.set_vexpand(true);
    reader_scroll.set_margin_start(18);
    reader_scroll.set_margin_end(18);
    reader_scroll.set_margin_bottom(18);
    reader_scroll.add_css_class("card");
    reader_scroll.set_child(Some(&reader));
    reader_body.append(&reader_scroll);
    let reader_page = adw::NavigationPage::new(&reader_body, "File Diff");
    navigation.add(&reader_page);
    dialog.set_child(Some(&navigation));

    let this = Rc::new(ChangesDialog {
        actor,
        workspace: workspace.into(),
        navigation: navigation.clone(),
        reader_page,
        rows,
        summary,
        path,
        status,
        reader,
        added_color,
        removed_color,
        row_map: RefCell::new(BTreeMap::new()),
        selected: RefCell::new(None),
        alive: Cell::new(true),
        summary_generation: Cell::new(0),
        detail_generation: Cell::new(0),
        style_handlers: RefCell::new(Vec::new()),
    });
    let keepalive = RefCell::new(Some(this.clone()));
    dialog.connect_closed(move |dialog| {
        if let Some(this) = keepalive.borrow_mut().take() {
            this.alive.set(false);
            this.invalidate_detail();
            dialog.set_child(None::<&gtk4::Widget>);
            let style = adw::StyleManager::default();
            for handler in this.style_handlers.borrow_mut().drain(..) {
                style.disconnect(handler);
            }
        }
    });
    for refresh in [list_refresh.clone(), reader_refresh] {
        let weak = Rc::downgrade(&this);
        refresh.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.refresh();
            }
        });
    }
    let weak = Rc::downgrade(&this);
    back.connect_clicked(move |_| {
        if let Some(this) = weak.upgrade() {
            this.navigation.pop();
        }
    });
    let weak = Rc::downgrade(&this);
    navigation.connect_popped(move |_, _| {
        if let Some(this) = weak.upgrade() {
            this.invalidate_detail();
            if let Some(row) = this
                .selected
                .borrow()
                .as_ref()
                .and_then(|path| this.row_map.borrow().get(path).cloned())
            {
                this.rows.select_row(Some(&row));
                row.grab_focus();
            }
        }
    });
    let keys = gtk4::EventControllerKey::new();
    let weak = Rc::downgrade(&this);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk4::gdk::Key::Left && modifiers.contains(gtk4::gdk::ModifierType::ALT_MASK) {
            if let Some(this) = weak.upgrade() {
                if this.in_reader() {
                    this.navigation.pop();
                    return gtk4::glib::Propagation::Stop;
                }
            }
        }
        gtk4::glib::Propagation::Proceed
    });
    navigation.add_controller(keys);
    let style = adw::StyleManager::default();
    let weak = Rc::downgrade(&this);
    let dark_handler = style.connect_dark_notify(move |_| {
        if let Some(this) = weak.upgrade() {
            this.update_colors();
        }
    });
    let weak = Rc::downgrade(&this);
    let contrast_handler = style.connect_high_contrast_notify(move |_| {
        if let Some(this) = weak.upgrade() {
            this.update_colors();
        }
    });
    this.style_handlers
        .borrow_mut()
        .extend([dark_handler, contrast_handler]);
    dialog.present(Some(window));
    dialog.set_focus(Some(&list_refresh));
    this.update_colors();
    this.refresh();
}

impl ChangesDialog {
    fn invalidate_detail(&self) {
        self.detail_generation
            .set(self.detail_generation.get().wrapping_add(1));
    }

    fn in_reader(&self) -> bool {
        self.navigation.visible_page().as_ref() == Some(&self.reader_page)
    }

    /// Updates syntax tag foreground and subtle paragraph background colors based on CSS probe widgets.
    fn update_colors(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        gtk4::glib::idle_add_local_once(move || {
            if let Some(this) = weak.upgrade().filter(|this| this.alive.get()) {
                for (tag, probe) in [
                    ("added", &this.added_color),
                    ("removed", &this.removed_color),
                ] {
                    if let Some(tag) = this.reader.buffer().tag_table().lookup(tag) {
                        let color = probe.color();
                        tag.set_foreground_rgba(Some(&color));
                        let mut bg = color;
                        bg.set_alpha(0.08);
                        tag.set_paragraph_background_rgba(Some(&bg));
                    }
                }
            }
        });
    }

    fn refresh(self: &Rc<Self>) {
        self.invalidate_detail();
        self.reader.buffer().set_text("");
        self.status.set_visible(true);
        self.status.set_text("Refreshing working tree changes…");
        self.summary.set_text("Loading working tree changes…");
        let generation = self.summary_generation.get().wrapping_add(1);
        self.summary_generation.set(generation);
        let this = self.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = this
                .actor
                .call(
                    signaltty_proto::method::WORKSPACE_DIFF,
                    json!({"workspace_id": this.workspace}),
                )
                .await
                .and_then(|value| {
                    serde_json::from_value::<Summary>(value).map_err(|e| e.to_string())
                });
            if !this.alive.get() || this.summary_generation.get() != generation {
                return;
            }
            match result {
                Ok(summary) => {
                    this.render_summary(summary);
                    if this.in_reader() {
                        let path = this.selected.borrow().clone();
                        if let Some(path) = path {
                            if this.row_map.borrow().contains_key(&path) {
                                this.load_file(&path);
                            } else {
                                this.status.set_text("This file is no longer in the working tree changes. Return to the list or refresh.");
                            }
                        }
                    }
                }
                Err(error) => {
                    this.summary.set_text(&format!(
                        "Couldn't read changes: {error}. Refresh to try again."
                    ));
                    this.status.set_text(&format!(
                        "Couldn't read changes: {error}. Refresh to try again."
                    ));
                }
            }
        });
    }

    fn render_summary(self: &Rc<Self>, summary: Summary) {
        self.summary.set_text(&format!(
            "Changes against HEAD · {} files · +{} −{}",
            summary.files.len(),
            summary.added,
            summary.removed
        ));
        let mut previous = std::mem::take(&mut *self.row_map.borrow_mut());
        let mut current = BTreeMap::new();
        for file in summary.files {
            let row = previous.remove(&file.path).unwrap_or_else(|| {
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&file.path);
                row.set_title_lines(0);
                row.set_activatable(true);
                let weak = Rc::downgrade(self);
                let path = file.path.clone();
                row.connect_activated(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.open_file(&path);
                    }
                });
                self.rows.append(&row);
                row
            });
            row.set_subtitle(&if file.untracked {
                "Untracked".into()
            } else if file.binary {
                "Binary".into()
            } else {
                format!("Tracked · +{} −{}", file.added, file.removed)
            });
            current.insert(file.path, row);
        }
        for row in previous.into_values() {
            self.rows.remove(&row);
        }
        // Remove only the non-file empty-state row, preserving mounted file rows.
        let mut child = self.rows.first_child();
        while let Some(row) = child {
            child = row.next_sibling();
            if row.widget_name() == "no-changes" {
                self.rows.remove(&row);
            }
        }
        if current.is_empty() {
            let row = adw::ActionRow::new();
            row.set_widget_name("no-changes");
            row.set_use_markup(false);
            row.set_title("No working tree changes");
            self.rows.append(&row);
        }
        *self.row_map.borrow_mut() = current;
        if let Some(row) = self
            .selected
            .borrow()
            .as_ref()
            .and_then(|path| self.row_map.borrow().get(path).cloned())
        {
            self.rows.select_row(Some(&row));
        }
    }

    fn open_file(self: &Rc<Self>, path: &str) {
        *self.selected.borrow_mut() = Some(path.into());
        if let Some(row) = self.row_map.borrow().get(path) {
            self.rows.select_row(Some(row));
        }
        self.path.set_text(path);
        if !self.in_reader() {
            self.navigation.push(&self.reader_page);
        }
        self.reader.grab_focus();
        self.load_file(path);
    }

    fn load_file(self: &Rc<Self>, path: &str) {
        self.invalidate_detail();
        let generation = self.detail_generation.get();
        self.reader.buffer().set_text("");
        self.status.set_visible(true);
        self.status.set_text("Loading file changes…");
        let path = path.to_owned();
        let this = self.clone();
        gtk4::glib::spawn_future_local(async move {
            let result = this
                .actor
                .call(
                    signaltty_proto::method::WORKSPACE_FILE_DIFF,
                    json!({"workspace_id": this.workspace, "path": path}),
                )
                .await
                .and_then(|value| {
                    serde_json::from_value::<FileDiff>(value).map_err(|e| e.to_string())
                });
            if !this.alive.get()
                || !this.in_reader()
                || this.detail_generation.get() != generation
                || this.selected.borrow().as_deref() != Some(&path)
            {
                return;
            }
            match result {
                Ok(diff) if diff.path == path => this.render_file(diff),
                Ok(_) => this
                    .status
                    .set_text("The server returned a different file. Refresh to try again."),
                Err(error) => this.status.set_text(&format!(
                    "Couldn't read this file: {error}. Refresh to try again."
                )),
            }
        });
    }

    fn render_file(&self, diff: FileDiff) {
        let buffer = self.reader.buffer();
        let untracked = if diff.untracked { "Untracked · " } else { "" };
        match diff.content {
            DiffContent::Binary => self.status.set_text(&format!(
                "{untracked}Binary file changed. No text preview is available."
            )),
            DiffContent::Unchanged => self
                .status
                .set_text("This file no longer has changes against HEAD."),
            DiffContent::Unavailable { reason } => {
                self.status.set_text(&format!("{untracked}{reason}"))
            }
            DiffContent::Text {
                hunks,
                truncated,
                notice,
            } => {
                let mut status = untracked.trim_end_matches(" · ").to_owned();
                if let Some(notice) = notice.or_else(|| {
                    truncated.then(|| "Incomplete preview: size or line limit reached.".into())
                }) {
                    if !status.is_empty() {
                        status.push_str(" · ");
                    }
                    status.push_str(&notice);
                }
                self.status.set_text(&status);
                let width = hunks
                    .iter()
                    .flat_map(|h| &h.lines)
                    .flat_map(|line| [line.old_line, line.new_line])
                    .flatten()
                    .max()
                    .unwrap_or(1)
                    .to_string()
                    .len();
                for hunk in hunks {
                    buffer.insert_with_tags_by_name(
                        &mut buffer.end_iter(),
                        &format!("{}\n", hunk.heading),
                        &["heading"],
                    );
                    for line in hunk.lines {
                        let old = line.old_line.map(|n| n.to_string()).unwrap_or_default();
                        let new = line.new_line.map(|n| n.to_string()).unwrap_or_default();
                        let (sign, tag) = match line.kind {
                            DiffLineKind::Context => (' ', None),
                            DiffLineKind::Added => ('+', Some("added")),
                            DiffLineKind::Removed => ('−', Some("removed")),
                            DiffLineKind::NoNewline => ('\\', None),
                        };
                        let text = format!("{old:>width$}  {new:>width$}  {sign} {}\n", line.text);
                        if let Some(tag) = tag {
                            buffer.insert_with_tags_by_name(&mut buffer.end_iter(), &text, &[tag]);
                        } else {
                            buffer.insert(&mut buffer.end_iter(), &text);
                        }
                    }
                    buffer.insert(&mut buffer.end_iter(), "\n");
                }
                buffer.place_cursor(&buffer.start_iter());
            }
        }
        self.status.set_visible(!self.status.text().is_empty());
    }
}
