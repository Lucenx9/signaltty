//! Vertical workspace sidebar: project/agent/doing/needs-me at a
//! glance. Rows show name, agent kinds, worst lifecycle, attention dot,
//! branch, latest message, and time since activity.

use gtk4::prelude::*;

use crate::util::{attention_css, time_ago};

pub struct WsSummary {
    pub id: String,
    pub name: String,
    pub agents: String,
    pub lifecycle: String,
    pub attention: String,
    pub branch: String,
    pub message: String,
    pub ago: String,
}

type SelectCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn(String)>>>>;

pub struct Sidebar {
    pub scrolled: gtk4::ScrolledWindow,
    list: gtk4::ListBox,
    on_select: SelectCallback,
}

impl Sidebar {
    pub fn new() -> Sidebar {
        let list = gtk4::ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Single);
        let on_select: SelectCallback = std::rc::Rc::new(std::cell::RefCell::new(None));
        {
            let on_select = std::rc::Rc::clone(&on_select);
            list.connect_row_selected(move |_, row| {
                if let Some(row) = row {
                    if let Some(cb) = on_select.borrow().as_ref() {
                        cb(row.widget_name().to_string());
                    }
                }
            });
        }
        let scrolled = gtk4::ScrolledWindow::new();
        scrolled.set_child(Some(&list));
        scrolled.set_min_content_width(264);
        scrolled.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        Sidebar {
            scrolled,
            list,
            on_select,
        }
    }

    pub fn set_on_select(&self, cb: impl Fn(String) + 'static) {
        *self.on_select.borrow_mut() = Some(Box::new(cb));
    }

    pub fn update(&self, items: &[WsSummary]) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for item in items {
            self.list.append(&Self::row(item));
        }
    }

    pub fn select(&self, ws_id: &str) {
        let mut child = self.list.first_child();
        while let Some(c) = child {
            if c.widget_name() == ws_id {
                if let Some(row) = c.downcast_ref::<gtk4::ListBoxRow>() {
                    self.list.select_row(Some(row));
                }
                return;
            }
            child = c.next_sibling();
        }
    }

    fn row(item: &WsSummary) -> gtk4::ListBoxRow {
        let row = gtk4::ListBoxRow::new();
        row.set_widget_name(&item.id);
        let vbox = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        vbox.set_margin_top(6);
        vbox.set_margin_bottom(6);
        vbox.set_margin_start(8);
        vbox.set_margin_end(8);

        let top = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let name = gtk4::Label::new(Some(&item.name));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.add_css_class("heading");
        let dot = gtk4::Label::new(Some("●"));
        dot.add_css_class("attention-dot");
        let css = attention_css(&item.attention);
        if !css.is_empty() {
            dot.add_css_class(css);
        } else {
            dot.set_opacity(0.15);
        }
        top.append(&name);
        top.append(&dot);

        let mid = gtk4::Label::new(Some(&format!(
            "{} · {} · {}",
            item.agents, item.lifecycle, item.branch
        )));
        mid.set_xalign(0.0);
        mid.add_css_class("dim");
        mid.add_css_class("caption");

        let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let msg = gtk4::Label::new(Some(&item.message));
        msg.set_xalign(0.0);
        msg.set_hexpand(true);
        msg.set_max_width_chars(32);
        msg.set_wrap(true);
        msg.set_wrap_mode(gtk4::pango::WrapMode::Char);
        msg.add_css_class("caption");
        let ago = gtk4::Label::new(Some(&item.ago));
        ago.add_css_class("dim");
        ago.add_css_class("caption");
        bottom.append(&msg);
        bottom.append(&ago);

        vbox.append(&top);
        vbox.append(&mid);
        vbox.append(&bottom);
        row.set_child(Some(&vbox));
        row
    }
}

/// Build a sidebar summary from a `workspace.get` result value.
pub fn summarize(
    ws: &serde_json::Value,
    tabs: &[serde_json::Value],
    panes: &[serde_json::Value],
) -> WsSummary {
    use std::collections::BTreeSet;
    let mut agents = BTreeSet::new();
    let mut worst_lc = ("unknown", 0u8);
    let mut worst_att = ("none", 0u8);
    let mut message = String::new();
    let mut latest = String::new();
    // Severity ranks mirror the core model.
    let lc_rank = |s: &str| match s {
        "blocked" => 5,
        "failed" => 4,
        "working" => 3,
        "done" => 2,
        "idle" => 1,
        _ => 0,
    };
    let att_rank = |s: &str| match s {
        "error" => 5,
        "permission_required" => 4,
        "input_required" => 3,
        "warning" => 2,
        "unread" => 1,
        _ => 0,
    };
    for p in panes {
        if let Some(kind) = p
            .get("agent")
            .and_then(|a| a.get("kind"))
            .and_then(|k| k.as_str())
        {
            if kind != "none" && kind != "generic" {
                agents.insert(kind.to_string());
            }
        }
        let lc = p
            .get("lifecycle")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if lc_rank(lc) > worst_lc.1 {
            worst_lc = (lc, lc_rank(lc));
        }
        let att = p
            .get("attention")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        if att_rank(att) > worst_att.1 {
            worst_att = (att, att_rank(att));
        }
        if message.is_empty() {
            if let Some(m) = p.get("last_message").and_then(|v| v.as_str()) {
                message = m.to_string();
            }
        }
        if let Some(ts) = p.get("last_activity_at").and_then(|v| v.as_str()) {
            if ts > latest.as_str() {
                latest = ts.to_string();
            }
        }
    }
    let _ = tabs;
    WsSummary {
        id: ws
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        name: ws
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string(),
        agents: if agents.is_empty() {
            "shell".to_string()
        } else {
            agents.into_iter().collect::<Vec<_>>().join("+")
        },
        lifecycle: worst_lc.0.to_string(),
        attention: worst_att.0.to_string(),
        branch: ws
            .get("git")
            .and_then(|g| g.get("branch"))
            .and_then(|b| b.as_str())
            .unwrap_or("-")
            .to_string(),
        message,
        ago: time_ago(&latest),
    }
}
