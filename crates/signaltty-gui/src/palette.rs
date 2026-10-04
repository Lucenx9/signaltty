use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use signaltty_core::Workspace;

use crate::actions::ACTIONS;

#[derive(Clone)]
enum Target {
    Action(String),
    Workspace(String),
}

#[derive(Clone)]
struct Choice {
    label: String,
    detail: String,
    target: Target,
}

/// Presents the command and workspace palette dialog over the specified application window.
pub fn present(
    window: &adw::ApplicationWindow,
    workspaces: &[Workspace],
    on_workspace: impl Fn(&str) + 'static,
    on_closed: impl Fn() + 'static,
) -> adw::Dialog {
    let dialog = adw::Dialog::new();
    dialog.set_title("Commands and Workspaces");
    dialog.set_content_width(560);
    dialog.set_content_height(480);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    body.append(&adw::HeaderBar::new());
    let entry = gtk4::SearchEntry::new();
    entry.set_placeholder_text(Some("Find a command or workspace"));
    entry.set_margin_start(18);
    entry.set_margin_end(18);
    body.append(&entry);
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_valign(gtk4::Align::Start);
    list.set_margin_start(18);
    list.set_margin_end(18);
    list.set_margin_bottom(18);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    body.append(&scroll);
    dialog.set_child(Some(&body));

    let choices: Vec<Choice> = workspaces
        .iter()
        .map(|ws| Choice {
            label: ws.name.clone(),
            detail: format!("Workspace · {}", ws.cwd),
            target: Target::Workspace(ws.id.clone()),
        })
        .chain(ACTIONS.iter().filter_map(|action| {
            action.label.map(|label| Choice {
                label: label.into(),
                detail: "Command".into(),
                target: Target::Action(format!("win.{}", action.name)),
            })
        }))
        .collect();
    let choices = Rc::new(choices);
    let filter_choices = choices.clone();
    let query = entry.downgrade();
    list.set_filter_func(move |row| {
        let Some(choice) = filter_choices.get(row.index() as usize) else {
            return false;
        };
        let Some(query) = query.upgrade() else {
            return false;
        };
        let needle = query.text().to_lowercase();
        choice.label.to_lowercase().contains(&needle)
            || choice.detail.to_lowercase().contains(&needle)
    });
    for choice in choices.iter() {
        let row = adw::ActionRow::new();
        row.set_use_markup(false);
        row.set_title(&choice.label);
        row.set_subtitle(&choice.detail);
        row.set_title_lines(2);
        row.set_subtitle_lines(2);
        row.set_activatable(true);
        list.append(&row);
    }
    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
    let targets: Vec<Target> = choices.iter().map(|choice| choice.target.clone()).collect();
    let filtered = list.clone();
    entry.connect_changed(move |_| {
        filtered.invalidate_filter();
        let first = (0..choices.len())
            .filter_map(|i| filtered.row_at_index(i as i32))
            .find(|r| r.is_child_visible());
        filtered.select_row(first.as_ref());
    });
    let weak_window = window.downgrade();
    let pending = Rc::new(RefCell::new(None));
    let selection = pending.clone();
    let close = dialog.downgrade();
    list.connect_row_activated(move |_, row| {
        let Some(target) = targets.get(row.index() as usize) else {
            return;
        };
        *selection.borrow_mut() = Some(target.clone());
        if let Some(close) = close.upgrade() {
            close.close();
        }
    });
    let selected = list.clone();
    entry.connect_activate(move |_| {
        if let Some(row) = selected.selected_row() {
            selected.emit_by_name::<()>("row-activated", &[&row]);
        }
    });
    let keys = gtk4::EventControllerKey::new();
    let selected = list.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        let direction = if key == gtk4::gdk::Key::Down {
            1
        } else if key == gtk4::gdk::Key::Up {
            -1
        } else {
            return gtk4::glib::Propagation::Proceed;
        };
        let current = selected.selected_row().map(|r| r.index()).unwrap_or(-1);
        let mut index = current + direction;
        while index >= 0 {
            let Some(row) = selected.row_at_index(index) else {
                break;
            };
            if row.is_child_visible() {
                selected.select_row(Some(&row));
                break;
            }
            index += direction;
        }
        gtk4::glib::Propagation::Stop
    });
    entry.add_controller(keys);
    let close = dialog.downgrade();
    entry.connect_stop_search(move |_| {
        if let Some(close) = close.upgrade() {
            close.close();
        }
    });
    dialog.connect_closed(move |_| {
        on_closed();
        match pending.borrow_mut().take() {
            Some(Target::Action(name)) => {
                if let Some(window) = weak_window.upgrade() {
                    let _ = gtk4::prelude::WidgetExt::activate_action(&window, &name, None);
                }
            }
            Some(Target::Workspace(id)) => on_workspace(&id),
            None => {}
        }
    });
    dialog.present(Some(window));
    entry.grab_focus();
    dialog
}
