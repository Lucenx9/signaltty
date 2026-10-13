//! Native project scope control. Selection filters navigation, never PTYs.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::prelude::*;
use signaltty_core::project::{CatalogAuthority, ProjectCatalog, ProjectId, ProjectScope};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScopeChange {
    User,
    Reconcile,
}

type Changed = RefCell<Option<Box<dyn Fn(ProjectScope, ScopeChange)>>>;

pub struct ProjectPicker {
    pub button: gtk4::MenuButton,
    label: gtk4::Label,
    context: gtk4::Label,
    popover: gtk4::Popover,
    search: gtk4::SearchEntry,
    list: gtk4::ListBox,
    empty: gtk4::Label,
    choices: RefCell<Vec<(ProjectScope, gtk4::ListBoxRow, gtk4::Image)>>,
    pub catalog: RefCell<ProjectCatalog>,
    scope: RefCell<ProjectScope>,
    changed: Changed,
}

impl ProjectPicker {
    pub fn new() -> Rc<Self> {
        let button = gtk4::MenuButton::new();
        button.add_css_class("project-selector");
        button.set_tooltip_text(Some("Choose Project"));
        let label = gtk4::Label::new(Some("All projects"));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let context = gtk4::Label::new(None);
        context.add_css_class("caption");
        context.add_css_class("dim-label");
        context.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        context.set_max_width_chars(12);
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        content.append(&decorative_icon("signaltty-folder-symbolic"));
        content.append(&label);
        content.append(&context);
        content.append(&decorative_icon("pan-down-symbolic"));
        button.set_child(Some(&content));

        let popover = gtk4::Popover::new();
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        let search = gtk4::SearchEntry::new();
        search.set_placeholder_text(Some("Find a project…"));
        search.update_property(&[gtk4::accessible::Property::Label("Find a project")]);
        let list = gtk4::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_max_content_height(360);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&list));
        let empty = gtk4::Label::new(Some("No matching projects"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(12);
        empty.set_margin_bottom(12);
        empty.set_visible(false);
        body.append(&search);
        body.append(&scroll);
        body.append(&empty);
        popover.set_child(Some(&body));
        button.set_popover(Some(&popover));

        let picker = Rc::new(Self {
            button,
            label,
            context,
            popover,
            search,
            list,
            empty,
            choices: RefCell::new(Vec::new()),
            catalog: RefCell::new(ProjectCatalog::default()),
            scope: RefCell::new(ProjectScope::All),
            changed: RefCell::new(None),
        });
        let weak = Rc::downgrade(&picker);
        picker.search.connect_search_changed(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.filter();
            }
        });
        let weak = Rc::downgrade(&picker);
        picker.list.connect_row_activated(move |_, row| {
            let Some(picker) = weak.upgrade() else { return };
            let scope = picker
                .choices
                .borrow()
                .iter()
                .find(|(_, candidate, _)| candidate == row)
                .map(|(scope, _, _)| scope.clone());
            if let Some(scope) = scope {
                picker.set_scope(scope, ScopeChange::User);
                picker.popover.popdown();
            }
        });
        let weak = Rc::downgrade(&picker);
        picker.search.connect_activate(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.filter();
                if let Some(row) = picker.first_match() {
                    picker.list.emit_by_name::<()>("row-activated", &[&row]);
                }
            }
        });
        let keys = gtk4::EventControllerKey::new();
        let weak = Rc::downgrade(&picker);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key != gtk4::gdk::Key::Down {
                return gtk4::glib::Propagation::Proceed;
            }
            if let Some(picker) = weak.upgrade() {
                picker.filter();
                if let Some(row) = picker.first_match() {
                    picker.list.select_row(Some(&row));
                    row.grab_focus();
                }
            }
            gtk4::glib::Propagation::Stop
        });
        picker.search.add_controller(keys);
        let dismiss = gtk4::EventControllerKey::new();
        dismiss.set_name(Some("project-dismiss"));
        dismiss.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let weak = Rc::downgrade(&picker);
        dismiss.connect_key_pressed(move |_, key, _, _| {
            if key != gtk4::gdk::Key::Escape {
                return gtk4::glib::Propagation::Proceed;
            }
            if let Some(picker) = weak.upgrade() {
                picker.popover.popdown();
            }
            gtk4::glib::Propagation::Stop
        });
        picker.popover.add_controller(dismiss);
        let weak = Rc::downgrade(&picker);
        picker.popover.connect_show(move |_| {
            if let Some(picker) = weak.upgrade() {
                if let Some(window) = picker
                    .button
                    .root()
                    .and_then(|root| root.downcast::<gtk4::Window>().ok())
                {
                    if let Some(body) = picker.popover.child() {
                        body.set_size_request((window.width() - 48).clamp(120, 320), -1);
                    }
                }
                picker.search.set_text("");
                picker.filter();
                picker.search.grab_focus();
            }
        });
        let weak = Rc::downgrade(&picker);
        picker.popover.connect_closed(move |_| {
            if let Some(picker) = weak.upgrade() {
                picker.button.grab_focus();
            }
        });
        picker.rebuild_choices();
        picker.update_label();
        picker
    }

    pub fn set_on_change(&self, callback: impl Fn(ProjectScope, ScopeChange) + 'static) {
        *self.changed.borrow_mut() = Some(Box::new(callback));
    }

    pub fn scope(&self) -> ProjectScope {
        self.scope.borrow().clone()
    }

    pub fn set_scope(&self, scope: ProjectScope, reason: ScopeChange) {
        if self.scope() == scope {
            return;
        }
        self.scope.replace(scope.clone());
        self.update_label();
        if let Some(callback) = self.changed.borrow().as_ref() {
            callback(scope, reason);
        }
    }

    pub fn set_catalog(&self, catalog: ProjectCatalog, authority: CatalogAuthority) {
        let next = catalog.reconcile_scope(&self.catalog.borrow(), &self.scope(), authority);
        let changed = catalog.projects() != self.catalog.borrow().projects();
        self.catalog.replace(catalog);
        if changed {
            self.rebuild_choices();
        }
        self.set_scope(next, ScopeChange::Reconcile);
        self.update_label();
    }

    pub fn reveal_workspace(&self, id: &str) {
        let scope = self.scope();
        let catalog = self.catalog.borrow();
        if catalog.includes_workspace(&scope, id) {
            return;
        }
        let target = catalog
            .project_for_workspace(id)
            .map(|project| ProjectScope::Project(project.id.clone()))
            .unwrap_or(ProjectScope::All);
        drop(catalog);
        self.set_scope(target, ScopeChange::Reconcile);
    }

    fn rebuild_choices(&self) {
        for (_, row, _) in self.choices.borrow_mut().drain(..) {
            self.list.remove(&row);
        }
        self.add_choice(ProjectScope::All, "All projects", None, None);
        for project in self.catalog.borrow().projects() {
            self.add_choice(
                ProjectScope::Project(project.id.clone()),
                &project.name,
                Some(project.id.directory()),
                Some(project.workspace_ids.len()),
            );
        }
        self.filter();
    }

    fn add_choice(
        &self,
        scope: ProjectScope,
        name: &str,
        path: Option<&str>,
        count: Option<usize>,
    ) {
        let row = gtk4::ListBoxRow::new();
        let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        for setter in [
            gtk4::Widget::set_margin_top,
            gtk4::Widget::set_margin_bottom,
            gtk4::Widget::set_margin_start,
            gtk4::Widget::set_margin_end,
        ] {
            setter(body.upcast_ref(), 8);
        }
        body.append(&decorative_icon("signaltty-folder-symbolic"));
        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = gtk4::Label::new(Some(name));
        title.set_xalign(0.0);
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        title.set_max_width_chars(28);
        text.append(&title);
        if let Some(path) = path {
            let subtitle = gtk4::Label::new(Some(&crate::util::tilde(path)));
            subtitle.add_css_class("caption");
            subtitle.add_css_class("dim-label");
            subtitle.set_xalign(0.0);
            subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
            subtitle.set_max_width_chars(28);
            text.append(&subtitle);
            row.set_tooltip_text(Some(path));
        }
        body.append(&text);
        if let Some(count) = count {
            let count_label = gtk4::Label::new(Some(&count.to_string()));
            count_label.add_css_class("dim-label");
            count_label.add_css_class("numeric");
            count_label.update_property(&[gtk4::accessible::Property::Label(&format!(
                "{count} workspaces"
            ))]);
            body.append(&count_label);
        }
        let check = decorative_icon("object-select-symbolic");
        body.append(&check);
        row.set_child(Some(&body));
        self.list.append(&row);
        self.choices.borrow_mut().push((scope, row, check));
    }

    fn update_label(&self) {
        let scope = self.scope();
        let title = scope
            .directory()
            .map(|path| ProjectId::new(path).name().to_owned())
            .unwrap_or_else(|| "All projects".into());
        self.label.set_text(&title);
        self.label.set_tooltip_text(scope.directory());
        let duplicate = self
            .catalog
            .borrow()
            .projects()
            .iter()
            .filter(|project| project.name == title)
            .count()
            > 1;
        let parent = scope
            .directory()
            .and_then(|path| Path::new(path).parent())
            .and_then(|path| path.to_str());
        self.context
            .set_text(&parent.map(crate::util::tilde).unwrap_or_default());
        self.context.set_visible(duplicate);
        self.context.set_tooltip_text(scope.directory());
        let accessible = scope
            .directory()
            .map(|path| format!("Choose Project: {title}, {path}"))
            .unwrap_or_else(|| format!("Choose Project: {title}"));
        self.button
            .update_property(&[gtk4::accessible::Property::Label(&accessible)]);
        for (choice, _, check) in self.choices.borrow().iter() {
            check.set_opacity(if choice == &scope { 1.0 } else { 0.0 });
        }
    }

    fn first_match(&self) -> Option<gtk4::ListBoxRow> {
        self.choices
            .borrow()
            .iter()
            .find(|(_, row, _)| row.is_visible())
            .map(|(_, row, _)| row.clone())
    }

    fn filter(&self) {
        let query = self.search.text().to_lowercase();
        let mut matches = 0;
        for (scope, row, _) in self.choices.borrow().iter() {
            let visible = match scope {
                ProjectScope::All => query.is_empty(),
                ProjectScope::Project(id) => id.directory().to_lowercase().contains(&query),
            };
            row.set_visible(visible);
            matches += usize::from(visible);
        }
        self.empty.set_visible(matches == 0);
    }
}

fn decorative_icon(name: &str) -> gtk4::Image {
    let image = gtk4::Image::from_icon_name(name);
    image.set_pixel_size(16);
    image.set_accessible_role(gtk4::AccessibleRole::Presentation);
    image
}

pub fn load(path: &Path) -> ProjectScope {
    let parsed = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ProjectScope>(&bytes).ok());
    match parsed {
        Some(ProjectScope::Project(id)) if !id.directory().is_empty() => {
            ProjectScope::Project(ProjectId::new(id.directory()))
        }
        _ => ProjectScope::All,
    }
}

pub fn save(path: &Path, scope: &ProjectScope) {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let staging = path.with_extension("json.tmp");
        std::fs::write(&staging, serde_json::to_vec(scope)?)?;
        std::fs::rename(staging, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!("Couldn't save project scope: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_storage_roundtrips_and_recovers_missing_corrupt_or_empty_files() {
        let path = std::env::temp_dir().join(format!(
            "signaltty-scope-storage-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), ProjectScope::All);
        let scope = ProjectScope::Project(ProjectId::new("/one/api/"));
        save(&path, &scope);
        assert_eq!(load(&path), scope);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::write(&path, b"broken json").unwrap();
        assert_eq!(load(&path), ProjectScope::All);
        std::fs::write(&path, br#"{"kind":"project","project":""}"#).unwrap();
        assert_eq!(load(&path), ProjectScope::All);
        save(&path, &ProjectScope::All);
        assert_eq!(load(&path), ProjectScope::All);
        std::fs::remove_file(path).unwrap();
    }
}
