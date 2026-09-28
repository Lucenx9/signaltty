//! App shell: sidebar + notebook of tabs + split layouts of VTE panes.
//! Pure client of the server API: refetches on events, renders state,
//! forwards input. Closing this window never touches running sessions.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use serde_json::{json, Value};

use signaltty_core::model::{Layout, Pane, SplitDir, Tab, Workspace};

use crate::actor::{IpcHandle, UiEvent, UiTx};
use crate::notif::Notifier;
use crate::sidebar::{self, Sidebar};
use crate::terminal::{PaneCallbacks, PaneWidget};
use crate::util::attention_css;

struct Model {
    workspaces: Vec<Workspace>,
    active_ws: Option<String>,
    tabs: Vec<Tab>,
    panes: HashMap<String, Pane>,
}

pub struct App {
    window: libadwaita::ApplicationWindow,
    toasts: libadwaita::ToastOverlay,
    sidebar: Sidebar,
    notebook: gtk4::Notebook,
    stack: gtk4::Stack,
    actor: IpcHandle,
    notifier: Notifier,
    model: RefCell<Model>,
    widgets: RefCell<HashMap<String, Rc<PaneWidget>>>,
    pages: RefCell<HashMap<String, gtk4::Widget>>,
    tab_layouts: RefCell<HashMap<String, String>>,
    gui_tab: RefCell<Option<String>>,
    focused_pane: RefCell<Option<String>>,
    /// Fresh splits awaiting their first positioned allocation.
    splits: RefCell<Vec<(glib::WeakRef<gtk4::Paned>, f32)>>,
    me: RefCell<Weak<App>>,
}

fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

fn att_rank(s: &str) -> u8 {
    match s {
        "error" => 5,
        "permission_required" => 4,
        "input_required" => 3,
        "warning" => 2,
        "unread" => 1,
        _ => 0,
    }
}

impl App {
    pub fn new(app: &libadwaita::Application, actor: IpcHandle, ui_tx: UiTx) -> Rc<App> {
        let window = libadwaita::ApplicationWindow::new(app);
        window.set_title(Some("signaltty"));
        window.set_default_size(1280, 800);

        let header = libadwaita::HeaderBar::new();
        let title = gtk4::Label::new(Some("signaltty"));
        title.add_css_class("title");
        header.set_title_widget(Some(&title));

        let btn_new_ws = gtk4::Button::from_icon_name("list-add-symbolic");
        btn_new_ws.set_tooltip_text(Some("New workspace"));
        let btn_new_tab = gtk4::Button::from_icon_name("tab-new-symbolic");
        btn_new_tab.set_tooltip_text(Some("New tab"));
        header.pack_start(&btn_new_ws);
        header.pack_start(&btn_new_tab);

        let btn_split_h = gtk4::Button::from_icon_name("view-split-horizontal-symbolic");
        btn_split_h.set_tooltip_text(Some("Split right"));
        let btn_split_v = gtk4::Button::from_icon_name("view-split-vertical-symbolic");
        btn_split_v.set_tooltip_text(Some("Split down"));
        let btn_close = gtk4::Button::from_icon_name("window-close-symbolic");
        btn_close.set_tooltip_text(Some("Close pane"));
        let btn_next = gtk4::Button::from_icon_name("go-next-symbolic");
        btn_next.set_tooltip_text(Some("Next unread"));
        btn_next.add_css_class("suggested-action");
        header.pack_end(&btn_next);
        header.pack_end(&btn_close);
        header.pack_end(&btn_split_v);
        header.pack_end(&btn_split_h);

        let sidebar = Sidebar::new();
        let notebook = gtk4::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_hexpand(true);
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        content.append(&sidebar.scrolled);
        content.append(&notebook);

        let empty = libadwaita::StatusPage::new();
        empty.set_title("No workspace open");
        empty.set_description(Some("Create a workspace to start running agents."));
        let empty_btn = gtk4::Button::with_label("New workspace");
        empty_btn.add_css_class("suggested-action");
        empty.set_child(Some(&empty_btn));

        let stack = gtk4::Stack::new();
        stack.add_named(&content, Some("main"));
        stack.add_named(&empty, Some("empty"));

        let toasts = libadwaita::ToastOverlay::new();
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&header);
        root.append(&stack);
        toasts.set_child(Some(&root));
        window.set_content(Some(&toasts));

        let app = Rc::new(App {
            window,
            toasts,
            sidebar,
            notebook,
            stack,
            actor,
            notifier: Notifier::new(ui_tx),
            model: RefCell::new(Model {
                workspaces: Vec::new(),
                active_ws: None,
                tabs: Vec::new(),
                panes: HashMap::new(),
            }),
            widgets: RefCell::new(HashMap::new()),
            pages: RefCell::new(HashMap::new()),
            tab_layouts: RefCell::new(HashMap::new()),
            gui_tab: RefCell::new(None),
            focused_pane: RefCell::new(None),
            splits: RefCell::new(Vec::new()),
            me: RefCell::new(Weak::new()),
        });
        app.me.replace(Rc::downgrade(&app));

        // Header actions.
        let w = app.weak();
        btn_new_ws.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_new_workspace();
            }
        });
        let w = app.weak();
        btn_new_tab.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_new_tab();
            }
        });
        let w = app.weak();
        btn_split_h.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_split("right");
            }
        });
        let w = app.weak();
        btn_split_v.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_split("down");
            }
        });
        let w = app.weak();
        btn_close.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_close_pane();
            }
        });
        let w = app.weak();
        btn_next.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.focus_next_unread();
            }
        });
        let w = app.weak();
        empty_btn.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_new_workspace();
            }
        });
        // Sidebar selection (guard: programmatic re-select is a no-op).
        let w = app.weak();
        app.sidebar.set_on_select(move |ws_id| {
            if let Some(a) = w.upgrade() {
                let active = a.model.borrow().active_ws.clone();
                if active.as_deref() != Some(ws_id.as_str()) {
                    a.show_workspace(&ws_id);
                }
            }
        });
        // Track the GUI-visible tab so refresh never yanks selection back.
        let w = app.weak();
        app.notebook.connect_switch_page(move |_, page, _| {
            if let Some(a) = w.upgrade() {
                for (id, p) in a.pages.borrow().iter() {
                    if p == page {
                        *a.gui_tab.borrow_mut() = Some(id.clone());
                        break;
                    }
                }
            }
        });
        // Size-sync tick: VTE sizes + fresh split positions. Cheap, and
        // quiet when nothing changed (gtk4 0.11: no size-allocate signal).
        let w = app.weak();
        glib::timeout_add_local(Duration::from_millis(250), move || {
            if let Some(a) = w.upgrade() {
                a.sync_sizes();
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });

        app
    }

    fn sync_sizes(&self) {
        for w in self.widgets.borrow().values() {
            w.sync_size();
        }
        self.splits.borrow_mut().retain(|(weak, ratio)| {
            let Some(paned) = weak.upgrade() else {
                return false;
            };
            let (w, h) = (paned.width(), paned.height());
            let total = match paned.orientation() {
                gtk4::Orientation::Vertical => h,
                _ => w,
            };
            if total > 100 {
                paned.set_position((total as f32 * ratio) as i32);
                false
            } else {
                true
            }
        });
    }

    fn weak(&self) -> Weak<App> {
        self.me.borrow().clone()
    }

    pub fn present(&self) {
        self.window.present();
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(libadwaita::Toast::new(msg));
    }

    // ---- data ----

    /// Full refetch: sidebar summaries + active workspace render.
    pub fn refresh(&self) {
        let list = match self.actor.call("workspace.list", json!({})) {
            Ok(v) => v,
            Err(e) => {
                self.toast(&format!("server error: {e}"));
                return;
            }
        };
        let workspaces: Vec<Workspace> = list
            .get("workspaces")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let mut items = Vec::new();
        for ws in &workspaces {
            if let Ok(g) = self
                .actor
                .call("workspace.get", json!({"workspace_id": ws.id}))
            {
                let tabs: Vec<Value> = g
                    .get("tabs")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                let panes: Vec<Value> = g
                    .get("panes")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                items.push(sidebar::summarize(&g["workspace"], &tabs, &panes));
            }
        }
        self.sidebar.update(&items);
        {
            let mut m = self.model.borrow_mut();
            m.workspaces = workspaces;
            let keep = m
                .active_ws
                .clone()
                .filter(|id| m.workspaces.iter().any(|w| &w.id == id));
            let pick = keep.or_else(|| m.workspaces.first().map(|w| w.id.clone()));
            m.active_ws = pick;
        }
        // Bind first: a match scrutinee borrow would live into the arms.
        let active = self.model.borrow().active_ws.clone();
        match active {
            Some(id) => self.show_workspace(&id),
            None => {
                self.model.borrow_mut().tabs.clear();
                self.model.borrow_mut().panes.clear();
                self.clear_notebook();
                self.prune_widgets(&HashSet::new());
                self.stack.set_visible_child_name("empty");
            }
        }
    }

    /// Render one workspace: tabs + panes + widgets + sidebar selection.
    pub fn show_workspace(&self, ws_id: &str) {
        let g = match self
            .actor
            .call("workspace.get", json!({"workspace_id": ws_id}))
        {
            Ok(v) => v,
            Err(e) => {
                self.toast(&format!("workspace.get failed: {e}"));
                return;
            }
        };
        let tabs: Vec<Tab> = g
            .get("tabs")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let panes: Vec<Pane> = g
            .get("panes")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        {
            let mut m = self.model.borrow_mut();
            m.active_ws = Some(ws_id.to_string());
            m.tabs = tabs;
            m.panes = panes.into_iter().map(|p| (p.id.clone(), p)).collect();
        }
        self.stack.set_visible_child_name("main");
        self.render_notebook();
        self.sidebar.select(ws_id);
    }

    // ---- layout rendering ----

    fn clear_notebook(&self) {
        while self.notebook.n_pages() > 0 {
            self.notebook.remove_page(Some(0));
        }
        self.pages.borrow_mut().clear();
        self.tab_layouts.borrow_mut().clear();
        *self.gui_tab.borrow_mut() = None;
    }

    /// Reconcile notebook pages with model tabs. Only rebuilds a tab's
    /// widget tree when its layout actually changed (split drags survive).
    fn render_notebook(&self) {
        let (tabs, server_active) = {
            let m = self.model.borrow();
            let active = m
                .workspaces
                .iter()
                .find(|w| Some(&w.id) == m.active_ws.as_ref())
                .and_then(|w| w.active_tab_id.clone());
            (m.tabs.clone(), active)
        };
        let ids: HashSet<String> = tabs.iter().map(|t| t.id.clone()).collect();
        // Collect first: iterating a borrowed clone would hold the RefCell.
        let stale: Vec<(String, gtk4::Widget)> = self
            .pages
            .borrow()
            .iter()
            .filter(|(id, _)| !ids.contains(*id))
            .map(|(id, page)| (id.clone(), page.clone()))
            .collect();
        for (id, page) in &stale {
            if let Some(n) = self.notebook.page_num(page) {
                self.notebook.remove_page(Some(n));
            }
            self.pages.borrow_mut().remove(id);
            self.tab_layouts.borrow_mut().remove(id);
        }
        for tab in &tabs {
            let sig = serde_json::to_string(&tab.layout).unwrap_or_default();
            let changed = self.tab_layouts.borrow().get(&tab.id) != Some(&sig);
            if !self.pages.borrow().contains_key(&tab.id) {
                let page = self.build_tab_page(tab);
                let label = self.tab_label(tab);
                self.notebook.append_page(&page, Some(&label));
                self.pages.borrow_mut().insert(tab.id.clone(), page);
                self.tab_layouts.borrow_mut().insert(tab.id.clone(), sig);
            } else if changed {
                let old = self.pages.borrow().get(&tab.id).cloned().unwrap();
                let pos = self.notebook.page_num(&old);
                let page = self.build_tab_page(tab);
                let label = self.tab_label(tab);
                if let Some(n) = pos {
                    self.notebook.remove_page(Some(n));
                    self.notebook.insert_page(&page, Some(&label), Some(n));
                }
                self.pages.borrow_mut().insert(tab.id.clone(), page);
                self.tab_layouts.borrow_mut().insert(tab.id.clone(), sig);
            } else if let Some(page) = self.pages.borrow().get(&tab.id).cloned() {
                // Title/attention may change without layout change.
                let label = self.tab_label(tab);
                self.notebook.set_tab_label(&page, Some(&label));
            }
        }
        // Selection: GUI tab wins; else server active; else first.
        let pick = self
            .gui_tab
            .borrow()
            .clone()
            .filter(|id| ids.contains(id))
            .or_else(|| server_active.filter(|id| ids.contains(id)))
            .or_else(|| tabs.first().map(|t| t.id.clone()));
        *self.gui_tab.borrow_mut() = pick.clone();
        if let Some(id) = pick {
            if let Some(page) = self.pages.borrow().get(&id).cloned() {
                if let Some(n) = self.notebook.page_num(&page) {
                    self.notebook.set_current_page(Some(n));
                }
            }
        }
        // Meta + GC.
        {
            let m = self.model.borrow();
            for (id, w) in self.widgets.borrow().iter() {
                if let Some(p) = m.panes.get(id) {
                    w.update_meta(p);
                }
            }
        }
        let live = self.collect_live_panes();
        self.prune_widgets(&live);
    }

    fn build_tab_page(&self, tab: &Tab) -> gtk4::Widget {
        match &tab.layout {
            None => {
                let page = libadwaita::StatusPage::new();
                page.set_title("Empty tab");
                page.set_description(Some("Spawn a terminal to get started."));
                let btn = gtk4::Button::with_label("New terminal");
                btn.add_css_class("suggested-action");
                let w = self.weak();
                let tab_id = tab.id.clone();
                btn.connect_clicked(move |_| {
                    if let Some(a) = w.upgrade() {
                        a.spawn_shell_in_tab(&tab_id);
                    }
                });
                page.set_child(Some(&btn));
                page.upcast()
            }
            Some(layout) => self.build_layout(layout),
        }
    }

    fn build_layout(&self, layout: &Layout) -> gtk4::Widget {
        match layout {
            Layout::Pane { pane_id } => self.widget_for(pane_id).frame.clone().upcast(),
            Layout::Split {
                dir,
                ratio,
                first,
                second,
            } => {
                let paned = gtk4::Paned::new(match dir {
                    SplitDir::Right => gtk4::Orientation::Horizontal,
                    SplitDir::Down => gtk4::Orientation::Vertical,
                });
                paned.set_start_child(Some(&self.build_layout(first)));
                paned.set_end_child(Some(&self.build_layout(second)));
                paned.set_shrink_start_child(false);
                paned.set_shrink_end_child(false);
                paned.set_wide_handle(true);
                // Position once allocated (see sync_sizes tick).
                self.splits.borrow_mut().push((paned.downgrade(), *ratio));
                paned.upcast()
            }
        }
    }

    fn widget_for(&self, pane_id: &str) -> Rc<PaneWidget> {
        if let Some(w) = self.widgets.borrow().get(pane_id) {
            return Rc::clone(w);
        }
        let w = self.weak();
        let w2 = self.weak();
        let widget = PaneWidget::new(
            pane_id,
            self.actor.clone(),
            PaneCallbacks {
                on_focus: Box::new(move |id| {
                    if let Some(a) = w.upgrade() {
                        a.on_pane_focused(id);
                    }
                }),
                on_resume: Box::new(move |id| {
                    if let Some(a) = w2.upgrade() {
                        a.resume_pane(id);
                    }
                }),
            },
        );
        if let Some(p) = self.model.borrow().panes.get(pane_id) {
            widget.update_meta(p);
        }
        self.widgets
            .borrow_mut()
            .insert(pane_id.to_string(), Rc::clone(&widget));
        widget
    }

    fn collect_live_panes(&self) -> HashSet<String> {
        let m = self.model.borrow();
        let mut out = HashSet::new();
        for t in &m.tabs {
            if let Some(l) = &t.layout {
                out.extend(l.panes());
            }
        }
        out
    }

    fn prune_widgets(&self, live: &HashSet<String>) {
        let gone: Vec<String> = self
            .widgets
            .borrow()
            .keys()
            .filter(|id| !live.contains(*id))
            .cloned()
            .collect();
        for id in gone {
            if let Some(w) = self.widgets.borrow_mut().remove(&id) {
                w.detach();
            }
        }
    }

    fn tab_label(&self, tab: &Tab) -> gtk4::Widget {
        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let m = self.model.borrow();
        let mut worst = "none";
        if let Some(l) = &tab.layout {
            for pid in l.panes() {
                if let Some(p) = m.panes.get(&pid) {
                    let a = p.attention.as_str();
                    if att_rank(a) > att_rank(worst) {
                        worst = a;
                    }
                }
            }
        }
        drop(m);
        let dot = gtk4::Label::new(Some("●"));
        dot.add_css_class("attention-dot");
        let css = attention_css(worst);
        if !css.is_empty() {
            dot.add_css_class(css);
        } else {
            dot.set_opacity(0.15);
        }
        let label = gtk4::Label::new(Some(&tab.title));
        let close = gtk4::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        let w = self.weak();
        let tab_id = tab.id.clone();
        close.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                a.action_close_tab(&tab_id);
            }
        });
        hbox.append(&dot);
        hbox.append(&label);
        hbox.append(&close);
        hbox.upcast()
    }

    // ---- events ----

    pub fn on_event(&self, ev: UiEvent) {
        match ev {
            UiEvent::PtyData { pane_id, data } => {
                if let Some(w) = self.widgets.borrow().get(&pane_id) {
                    w.feed(&data);
                }
            }
            UiEvent::FocusPane(id) => {
                self.window.present();
                self.focus_pane(&id);
            }
            UiEvent::Reconnected => {
                self.toast("reconnected to server");
                self.refresh();
            }
            UiEvent::Disconnected => {
                self.toast("server connection lost — retrying");
            }
            UiEvent::ServerEvent { name, payload, .. } => {
                self.maybe_notify(&name, &payload);
                self.refresh();
            }
        }
    }

    /// Desktop notification for attention. Only on attention events:
    /// `notification.created` always precedes its attention raise, so
    /// notifying on both would double-notify.
    fn maybe_notify(&self, name: &str, payload: &Value) {
        if name != "attention.created" && name != "attention.updated" {
            return;
        }
        let pane_id = payload
            .get("pane_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let att = payload
            .get("attention")
            .and_then(|v| v.as_str())
            .unwrap_or("none");
        if pane_id.is_empty() || att == "none" {
            return;
        }
        if self.is_pane_visible(pane_id) {
            return;
        }
        let (title, msg) = match self.actor.call("pane.get", json!({"pane_id": pane_id})) {
            Ok(v) => {
                let p = &v["pane"];
                (
                    p["title"].as_str().unwrap_or("signaltty").to_string(),
                    p["last_message"].as_str().unwrap_or(att).to_string(),
                )
            }
            Err(_) => ("signaltty".to_string(), att.to_string()),
        };
        self.notifier.notify_attention(&title, &msg, pane_id);
    }

    fn is_pane_visible(&self, pane_id: &str) -> bool {
        self.focused_pane.borrow().as_deref() == Some(pane_id) && self.window.is_active()
    }

    fn on_pane_focused(&self, pane_id: &str) {
        *self.focused_pane.borrow_mut() = Some(pane_id.to_string());
    }

    // ---- focus navigation ----

    pub fn focus_pane(&self, pane_id: &str) {
        let pane: Option<Pane> = self
            .actor
            .call("pane.get", json!({"pane_id": pane_id}))
            .ok()
            .and_then(|v| serde_json::from_value(v["pane"].clone()).ok());
        let Some(pane) = pane else { return };
        // Bind first: an if-condition borrow would live into the body.
        let same_ws = self.model.borrow().active_ws.as_deref() == Some(pane.workspace_id.as_str());
        if !same_ws {
            self.show_workspace(&pane.workspace_id);
        }
        *self.gui_tab.borrow_mut() = Some(pane.tab_id.clone());
        if let Some(page) = self.pages.borrow().get(&pane.tab_id).cloned() {
            if let Some(n) = self.notebook.page_num(&page) {
                self.notebook.set_current_page(Some(n));
            }
        }
        if let Some(w) = self.widgets.borrow().get(pane_id) {
            w.focus();
        }
    }

    fn focus_next_unread(&self) {
        match self.actor.call("focus.next_unread", json!({})) {
            Ok(v) => match v.get("pane_id").and_then(|p| p.as_str()) {
                Some(id) => self.focus_pane(id),
                None => self.toast("No unread panes"),
            },
            Err(e) => self.toast(&format!("focus.next_unread failed: {e}")),
        }
    }

    // ---- actions ----

    fn active_ws_id(&self) -> Option<String> {
        self.model.borrow().active_ws.clone()
    }

    fn current_pane_id(&self) -> Option<String> {
        let m = self.model.borrow();
        if let Some(f) = m
            .active_ws
            .as_ref()
            .and_then(|_| self.focused_pane.borrow().clone())
            .filter(|id| m.panes.contains_key(id))
        {
            return Some(f);
        }
        let tab_id = self.gui_tab.borrow().clone()?;
        let tab = m.tabs.iter().find(|t| t.id == *tab_id)?;
        tab.active_pane_id.clone().or_else(|| {
            tab.layout
                .as_ref()
                .map(|l| l.panes())
                .and_then(|mut v| v.pop())
        })
    }

    fn action_new_workspace(&self) {
        let ws_id = match self
            .actor
            .call("workspace.create", json!({"name": "workspace"}))
        {
            Ok(v) => v["workspace"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            Err(e) => {
                self.toast(&format!("workspace.create failed: {e}"));
                return;
            }
        };
        if ws_id.is_empty() {
            self.toast("workspace.create returned no id");
            return;
        }
        // No active tab yet: pane.spawn auto-creates the "agents" tab.
        if let Err(e) = self.actor.call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": [user_shell()]}),
        ) {
            self.toast(&format!("pane.spawn failed: {e}"));
            return;
        }
        self.refresh();
        self.show_workspace(&ws_id);
    }

    fn action_new_tab(&self) {
        let Some(ws) = self.active_ws_id() else {
            self.action_new_workspace();
            return;
        };
        let tab_id = match self
            .actor
            .call("tab.create", json!({"workspace_id": ws, "title": "shell"}))
        {
            Ok(v) => v["tab"]["id"].as_str().unwrap_or_default().to_string(),
            Err(e) => {
                self.toast(&format!("tab.create failed: {e}"));
                return;
            }
        };
        if let Err(e) = self.actor.call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": tab_id, "argv": [user_shell()]}),
        ) {
            self.toast(&format!("pane.spawn failed: {e}"));
        }
        self.refresh();
    }

    fn spawn_shell_in_tab(&self, tab_id: &str) {
        let Some(ws) = self.active_ws_id() else {
            return;
        };
        if let Err(e) = self.actor.call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": tab_id, "argv": [user_shell()]}),
        ) {
            self.toast(&format!("pane.spawn failed: {e}"));
        }
        self.refresh();
    }

    fn action_split(&self, direction: &str) {
        let Some(pane_id) = self.current_pane_id() else {
            self.toast("No pane to split");
            return;
        };
        if let Err(e) = self.actor.call(
            "pane.split",
            json!({"pane_id": pane_id, "direction": direction}),
        ) {
            self.toast(&format!("pane.split failed: {e}"));
        }
        self.refresh();
    }

    fn action_close_pane(&self) {
        let Some(pane_id) = self.current_pane_id() else {
            self.toast("No pane to close");
            return;
        };
        if let Err(e) = self.actor.call("pane.close", json!({"pane_id": pane_id})) {
            self.toast(&format!("pane.close failed: {e}"));
        }
        self.refresh();
    }

    fn action_close_tab(&self, tab_id: &str) {
        if let Err(e) = self.actor.call("tab.close", json!({"tab_id": tab_id})) {
            self.toast(&format!("tab.close failed: {e}"));
        }
        self.refresh();
    }

    fn resume_pane(&self, pane_id: &str) {
        if let Err(e) = self.actor.call("pane.resume", json!({"pane_id": pane_id})) {
            self.toast(&format!("pane.resume failed: {e}"));
        }
        self.refresh();
    }
}
