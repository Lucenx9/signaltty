//! App shell. Pure client of the server API: refetches on events,
//! renders state, forwards input. Closing this window never touches
//! running sessions.
//!
//! ```text
//! AdwOverlaySplitView
//! ├─ sidebar  AdwToolbarView: [+ Workspaces] / workspace rows
//! └─ content  AdwToolbarView
//!    ├─ header  [sidebar] workspace · path       [● 2] [tab+] [menu]
//!    ├─ banner  (server connection lost)
//!    ├─ AdwTabBar (autohides with one tab)
//!    └─ AdwTabView → per tab: Bin.tab-page → Paned splits → pane cards
//! ```
//!
//! Every command is a `win.*` action with an accelerator, reachable
//! from the primary menu; widgets are reconciled in place so state
//! changes animate instead of flashing.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use gtk4::prelude::*;
use gtk4::{gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use serde_json::{json, Value};

use signaltty_core::{Attention, Layout, Lifecycle, Pane, SplitDir, Tab};

use crate::actor::{IpcHandle, UiEvent, UiTx};
use crate::notif::Notifier;
use crate::refresh::{PendingRefresh, WorkspaceCache};
use crate::sidebar::{self, Sidebar};
use crate::status;
use crate::terminal::{PaneAction, PaneCallbacks, PaneWidget};
use crate::util::tilde;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

struct Model {
    cache: WorkspaceCache,
    active_ws: Option<String>,
    tabs: Vec<Tab>,
    panes: HashMap<String, Pane>,
}

struct TabEntry {
    page: adw::TabPage,
    bin: adw::Bin,
    layout: Option<Layout>,
}

/// Weak divider widgets keyed like [`crate::dividers::Dividers`].
type PanedWidgets = HashMap<(String, Vec<bool>), glib::WeakRef<gtk4::Paned>>;

/// Header button: how many panes need you; click jumps to the next.
struct AttentionButton {
    revealer: gtk4::Revealer,
    button: gtk4::Button,
    dot: gtk4::Box,
    count: gtk4::Label,
}

pub struct App {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    split_view: adw::OverlaySplitView,
    title: adw::WindowTitle,
    sidebar: Sidebar,
    tab_view: adw::TabView,
    content: gtk4::Stack,
    attention: AttentionButton,
    actor: IpcHandle,
    notifier: Notifier,
    model: RefCell<Model>,
    pending_refresh: RefCell<PendingRefresh>,
    refresh_scheduled: Cell<bool>,
    widgets: RefCell<HashMap<String, Rc<PaneWidget>>>,
    tabs: RefCell<HashMap<String, TabEntry>>,
    /// Set while the GUI itself mutates the tab view, so selection
    /// changes it causes are not mistaken for the user's choice.
    reconciling: Cell<bool>,
    gui_tab: RefCell<Option<String>>,
    focused_pane: RefCell<Option<String>>,
    /// Divider state machine (ratios, drags, echo suppression);
    /// widgets live separately in `paned_widgets`.
    dividers: crate::dividers::Dividers,
    /// Weak divider widgets keyed like the state machine; App feeds
    /// observations from these and executes its commands on them.
    paned_widgets: RefCell<PanedWidgets>,
    /// The New Workspace dialog is single-instance: repeats of the
    /// action (or its accelerator) while it is open are ignored.
    new_ws_open: Cell<bool>,
    me: RefCell<Weak<App>>,
}

pub(crate) fn user_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

impl App {
    pub fn new(application: &adw::Application, actor: IpcHandle, ui_tx: UiTx) -> Rc<App> {
        let window = adw::ApplicationWindow::new(application);
        window.set_title(Some("signaltty"));
        window.set_default_size(1280, 800);
        window.set_size_request(360, 400);

        // ---- sidebar ----
        let sidebar = Sidebar::new();
        let sidebar_header = adw::HeaderBar::new();
        sidebar_header.set_title_widget(Some(&adw::WindowTitle::new("Workspaces", "")));
        let btn_new_ws = gtk4::Button::from_icon_name("list-add-symbolic");
        btn_new_ws.set_tooltip_text(Some("New Workspace (Ctrl+Shift+N)"));
        btn_new_ws.set_action_name(Some("win.new-workspace"));
        sidebar_header.pack_start(&btn_new_ws);
        let sidebar_page = adw::ToolbarView::new();
        sidebar_page.add_top_bar(&sidebar_header);
        sidebar_page.set_content(Some(&sidebar.widget));

        // ---- content header ----
        let title = adw::WindowTitle::new("signaltty", "");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let btn_sidebar = gtk4::ToggleButton::new();
        btn_sidebar.set_icon_name("sidebar-show-symbolic");
        btn_sidebar.set_tooltip_text(Some("Toggle Sidebar (F9)"));
        btn_sidebar.set_action_name(Some("win.toggle-sidebar"));
        header.pack_start(&btn_sidebar);
        let btn_menu = gtk4::MenuButton::new();
        btn_menu.set_icon_name("open-menu-symbolic");
        btn_menu.set_tooltip_text(Some("Main Menu"));
        btn_menu.set_menu_model(Some(&crate::actions::primary_menu()));
        btn_menu.set_primary(true);
        header.pack_end(&btn_menu);
        let btn_new_tab = gtk4::Button::from_icon_name("tab-new-symbolic");
        btn_new_tab.set_tooltip_text(Some("New Tab (Ctrl+Shift+T)"));
        btn_new_tab.set_action_name(Some("win.new-tab"));
        header.pack_end(&btn_new_tab);
        let attention = Self::attention_button();
        header.pack_end(&attention.revealer);

        let banner = adw::Banner::new("Lost connection to the session server — retrying…");

        // ---- tabs + empty states ----
        let tab_view = adw::TabView::new();
        let tab_bar = adw::TabBar::new();
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_autohide(true);

        let no_workspace = adw::StatusPage::new();
        no_workspace.set_icon_name(Some("utilities-terminal-symbolic"));
        no_workspace.set_title("No Workspaces");
        no_workspace.set_description(Some(
            "A workspace groups the agents working on one project. \
             They keep running when this window closes.",
        ));
        let btn_empty = gtk4::Button::with_label("New Workspace");
        btn_empty.add_css_class("pill");
        btn_empty.add_css_class("suggested-action");
        btn_empty.set_halign(gtk4::Align::Center);
        btn_empty.set_action_name(Some("win.new-workspace"));
        no_workspace.set_child(Some(&btn_empty));

        let no_tabs = adw::StatusPage::new();
        no_tabs.set_icon_name(Some("tab-new-symbolic"));
        no_tabs.set_title("No Tabs");
        no_tabs.set_description(Some("Open a tab to start a terminal in this workspace."));
        let btn_no_tabs = gtk4::Button::with_label("New Tab");
        btn_no_tabs.add_css_class("pill");
        btn_no_tabs.add_css_class("suggested-action");
        btn_no_tabs.set_halign(gtk4::Align::Center);
        btn_no_tabs.set_action_name(Some("win.new-tab"));
        no_tabs.set_child(Some(&btn_no_tabs));

        let content = gtk4::Stack::new();
        content.set_transition_type(gtk4::StackTransitionType::Crossfade);
        content.set_transition_duration(150);
        content.add_named(&tab_view, Some("tabs"));
        content.add_named(&no_tabs, Some("no-tabs"));
        content.add_named(&no_workspace, Some("no-workspace"));

        let content_page = adw::ToolbarView::new();
        content_page.add_top_bar(&header);
        content_page.add_top_bar(&banner);
        content_page.add_top_bar(&tab_bar);
        content_page.set_content(Some(&content));

        let split_view = adw::OverlaySplitView::new();
        split_view.set_sidebar(Some(&sidebar_page));
        split_view.set_content(Some(&content_page));
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        split_view.set_sidebar_width_fraction(0.24);

        // Narrow windows: the sidebar overlays instead of squeezing panes.
        let narrow = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 760sp").expect("breakpoint"),
        );
        narrow.add_setter(&split_view, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(narrow);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split_view));
        window.set_content(Some(&toasts));

        let app = Rc::new(App {
            window,
            toasts,
            banner,
            split_view,
            title,
            sidebar,
            tab_view,
            content,
            attention,
            actor,
            notifier: Notifier::new(ui_tx),
            model: RefCell::new(Model {
                cache: WorkspaceCache::default(),
                active_ws: None,
                tabs: Vec::new(),
                panes: HashMap::new(),
            }),
            pending_refresh: RefCell::new(PendingRefresh::default()),
            refresh_scheduled: Cell::new(false),
            widgets: RefCell::new(HashMap::new()),
            tabs: RefCell::new(HashMap::new()),
            reconciling: Cell::new(false),
            gui_tab: RefCell::new(None),
            focused_pane: RefCell::new(None),
            dividers: crate::dividers::Dividers::new(),
            paned_widgets: RefCell::new(HashMap::new()),
            new_ws_open: Cell::new(false),
            me: RefCell::new(Weak::new()),
        });
        app.me.replace(Rc::downgrade(&app));
        app.install_actions(application);
        app.connect_signals();
        app
    }

    fn attention_button() -> AttentionButton {
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        dot.set_valign(gtk4::Align::Center);
        let count = gtk4::Label::new(None);
        count.add_css_class("numeric");
        count.add_css_class("heading");
        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        inner.append(&dot);
        inner.append(&count);
        let button = gtk4::Button::new();
        button.set_child(Some(&inner));
        button.add_css_class("flat");
        button.add_css_class("attention-button");
        button.set_action_name(Some("win.next-attention"));
        let revealer = gtk4::Revealer::new();
        revealer.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
        revealer.set_transition_duration(150);
        revealer.set_child(Some(&button));
        AttentionButton {
            revealer,
            button,
            dot,
            count,
        }
    }

    fn install_actions(&self, application: &adw::Application) {
        // Behavior behind the registry's wiring; weak upgrades make
        // each callback a no-op after teardown, like PaneCallbacks.
        let method = |f: fn(&App)| {
            let w = self.weak();
            Box::new(move || {
                if let Some(a) = w.upgrade() {
                    f(&a);
                }
            }) as Box<dyn Fn()>
        };
        let split = |dir: SplitDir| {
            let w = self.weak();
            Box::new(move || {
                if let Some(a) = w.upgrade() {
                    a.action_split(dir);
                }
            }) as Box<dyn Fn()>
        };
        crate::actions::install(
            &self.window,
            application,
            &self.split_view,
            crate::actions::ActionHandlers {
                new_workspace: method(App::action_new_workspace),
                new_tab: method(App::action_new_tab),
                split_right: split(SplitDir::Right),
                split_down: split(SplitDir::Down),
                close_pane: method(App::action_close_pane),
                next_attention: method(App::focus_next_unread),
                about: method(App::show_about),
            },
        );
    }

    fn connect_signals(&self) {
        // Sidebar selection (guard: programmatic re-select is a no-op).
        let w = self.weak();
        self.sidebar.set_on_select(move |ws_id| {
            if let Some(a) = w.upgrade() {
                let active = a.model.borrow().active_ws.clone();
                if active.as_deref() != Some(ws_id.as_str()) {
                    a.show_workspace(&ws_id);
                }
                if a.split_view.is_collapsed() {
                    a.split_view.set_show_sidebar(false);
                }
            }
        });
        // Track the tab the user is looking at, so refreshes never yank
        // selection back to the server's idea of the active tab.
        let w = self.weak();
        self.tab_view.connect_selected_page_notify(move |view| {
            let Some(a) = w.upgrade() else { return };
            if a.reconciling.get() {
                return;
            }
            if let Some(page) = view.selected_page() {
                if let Some(id) = a.tab_id_of(&page) {
                    *a.gui_tab.borrow_mut() = Some(id);
                }
            }
        });
        // Closing a tab from the tab bar closes it on the server. Pages
        // the reconciler removes are no longer in `tabs` by the time
        // this runs, so they just close.
        let w = self.weak();
        self.tab_view.connect_close_page(move |view, page| {
            let closed = w.upgrade().and_then(|a| {
                let id = a.tab_id_of(page)?;
                a.tabs.borrow_mut().remove(&id);
                Some((a, id))
            });
            view.close_page_finish(page, true);
            if let Some((a, id)) = closed {
                if let Err(e) = a.actor.call("tab.close", json!({"tab_id": id})) {
                    a.toast(&format!("Couldn't close the tab — {e}"));
                }
                a.refresh_later();
            }
            glib::Propagation::Stop
        });
        // Terminals follow the desktop's colour scheme and mono font.
        let sm = adw::StyleManager::default();
        let w = self.weak();
        sm.connect_dark_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.restyle_terminals();
            }
        });
        let w = self.weak();
        sm.connect_monospace_font_name_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.restyle_terminals();
            }
        });
        // Size-sync tick: VTE sizes + fresh split positions. Cheap, and
        // quiet when nothing changed (gtk4 0.11: no size-allocate signal).
        let w = self.weak();
        glib::timeout_add_local(Duration::from_millis(250), move || match w.upgrade() {
            Some(a) => {
                a.sync_sizes();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        // Relative times in the sidebar age between events.
        let w = self.weak();
        glib::timeout_add_seconds_local(30, move || match w.upgrade() {
            Some(a) => {
                a.sidebar.refresh_times();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }

    fn restyle_terminals(&self) {
        for w in self.widgets.borrow().values() {
            w.apply_style();
        }
    }

    fn sync_sizes(&self) {
        for w in self.widgets.borrow().values() {
            w.sync_size();
        }
        self.sync_paneds();
    }

    /// One sizing pass: snapshot live dividers (pruning dead widget
    /// refs), ask the divider module what to do, and execute its
    /// commands with no borrows held (moves notify synchronously,
    /// sends block the UI thread).
    fn sync_paneds(&self) {
        let mut live = Vec::new();
        self.paned_widgets.borrow_mut().retain(|key, weak| {
            let Some(paned) = weak.upgrade() else {
                return false;
            };
            let total = match paned.orientation() {
                gtk4::Orientation::Vertical => paned.height(),
                _ => paned.width(),
            };
            live.push((key.0.clone(), key.1.clone(), total));
            true
        });
        let commands = self.dividers.tick(&live, Instant::now());
        for command in commands {
            match command {
                crate::dividers::Command::Place {
                    tab_id,
                    path,
                    position_px,
                } => self.place_paned(&tab_id, &path, position_px),
                crate::dividers::Command::Send {
                    tab_id,
                    path,
                    ratio,
                } => self.send_ratio(&tab_id, &path, ratio),
            }
        }
    }

    /// Move a divider to a server ratio without tripping the drag
    /// detector.
    fn place_paned(&self, tab_id: &str, path: &[bool], position: i32) {
        let key = (tab_id.to_string(), path.to_vec());
        let paned = self
            .paned_widgets
            .borrow()
            .get(&key)
            .and_then(|w| w.upgrade());
        let Some(paned) = paned else { return };
        self.dividers.suppressing(|| paned.set_position(position));
        self.dividers.placed(tab_id, path);
    }

    /// Persist a rested drag, reporting the outcome back to the divider
    /// module (echoed ratio, or staleness for a failed send).
    fn send_ratio(&self, tab_id: &str, path: &[bool], ratio: f32) {
        let ipath: Vec<u8> = path.iter().map(|b| u8::from(*b)).collect();
        match self.actor.call(
            "tab.set_ratio",
            json!({"tab_id": tab_id, "path": ipath, "ratio": ratio}),
        ) {
            Ok(v) => {
                let confirmed = v
                    .get("tab")
                    .and_then(|t| t.get("layout"))
                    .and_then(|l| serde_json::from_value::<Layout>(l.clone()).ok())
                    .and_then(|l| l.ratio_at_path(path))
                    .unwrap_or(ratio);
                self.dividers.send_succeeded(tab_id, path, confirmed);
            }
            Err(_) => {
                let alive = self
                    .model
                    .borrow()
                    .tabs
                    .iter()
                    .find(|t| t.id == tab_id)
                    .and_then(|t| t.layout.as_ref())
                    .is_some_and(|l| l.has_split_at(path));
                self.dividers.send_failed(tab_id, path, alive);
            }
        }
    }

    /// A divider moved; hand the observation to the divider module.
    fn on_paned_position(&self, tab_id: &str, path: &[bool], paned: &gtk4::Paned) {
        let total = match paned.orientation() {
            gtk4::Orientation::Vertical => paned.height(),
            _ => paned.width(),
        };
        self.dividers
            .position_changed(tab_id, path, paned.position(), total, Instant::now());
    }

    /// Forget a tab's dividers, state and widgets alike.
    fn drop_paneds(&self, tab_id: &str) {
        self.dividers.drop_tab(tab_id);
        self.paned_widgets
            .borrow_mut()
            .retain(|key, _| key.0 != tab_id);
    }

    fn weak(&self) -> Weak<App> {
        self.me.borrow().clone()
    }

    pub fn present(&self) {
        self.window.present();
    }

    fn toast(&self, msg: &str) {
        self.toasts.add_toast(adw::Toast::new(msg));
    }

    fn refresh_later(&self) {
        if let Some(id) = self.active_ws_id() {
            self.pending_refresh.borrow_mut().workspaces.insert(id);
        }
        self.schedule_refresh();
    }

    fn schedule_refresh(&self) {
        if self.pending_refresh.borrow().is_empty() || self.refresh_scheduled.replace(true) {
            return;
        }
        let w = self.weak();
        // One bounded batch per frame, also while the window is unmapped.
        // Further events merge into this batch without postponing its deadline.
        glib::timeout_add_local_once(Duration::from_millis(16), move || {
            if let Some(a) = w.upgrade() {
                let pending = a.pending_refresh.take();
                a.apply_refresh(pending);
                a.refresh_scheduled.set(false);
            }
        });
    }

    // ---- data ----

    /// Initial load, reconnect, or workspace creation/deletion only.
    pub fn refresh(&self) {
        self.apply_refresh(PendingRefresh::full());
    }

    fn refresh_workspace(&self, id: &str) {
        let mut pending = PendingRefresh::default();
        pending.workspaces.insert(id.to_string());
        self.apply_refresh(pending);
    }

    fn refresh_pane_workspace(&self, pane_id: &str) {
        let mut pending = PendingRefresh::default();
        pending.on_event(
            &self.model.borrow().cache,
            "pane.updated",
            &json!({"pane_id": pane_id}),
        );
        self.apply_refresh(pending);
    }

    fn apply_refresh(&self, pending: PendingRefresh) {
        let full = pending.full;
        let (changed, errors) = self
            .model
            .borrow_mut()
            .cache
            .refresh(pending, |method, params| self.actor.call(method, params));
        for error in errors {
            self.toast(&format!("Couldn't load workspaces — {error}"));
        }
        let (items, needing, active) = {
            let mut m = self.model.borrow_mut();
            let keep = m
                .active_ws
                .clone()
                .filter(|id| m.cache.workspaces.iter().any(|ws| &ws.id == id));
            m.active_ws = keep.or_else(|| m.cache.workspaces.first().map(|ws| ws.id.clone()));
            let mut items = Vec::new();
            let mut needing = Vec::new();
            for ws in &m.cache.workspaces {
                if let Some(snapshot) = m.cache.snapshots.get(&ws.id) {
                    items.push(sidebar::summarize(&snapshot.workspace, &snapshot.panes));
                    needing.extend(
                        snapshot
                            .panes
                            .iter()
                            .filter(|p| p.attention.needs_human())
                            .map(|p| p.attention),
                    );
                }
            }
            (items, needing, m.active_ws.clone())
        };
        let mut items = items;
        sidebar::sort_summaries(&mut items);
        self.sidebar.update(&items);
        self.update_attention_button(&needing);
        match active {
            Some(id) if full || changed.contains(&id) => self.show_workspace(&id),
            Some(_) => {}
            None => {
                {
                    let mut m = self.model.borrow_mut();
                    m.tabs.clear();
                    m.panes.clear();
                }
                self.render_tabs();
                self.title.set_title("signaltty");
                self.title.set_subtitle("");
                self.content.set_visible_child_name("no-workspace");
            }
        }
    }

    fn update_attention_button(&self, needing: &[Attention]) {
        let b = &self.attention;
        let worst = needing.iter().fold(Attention::None, |acc, a| acc.raise(*a));
        b.count.set_text(&needing.len().to_string());
        status::set_attention_class(&b.dot, worst);
        b.button.set_tooltip_text(Some(&match needing.len() {
            1 => "1 pane needs attention — jump to it (Ctrl+Shift+J)".to_string(),
            n => format!("{n} panes need attention — jump to the next (Ctrl+Shift+J)"),
        }));
        b.revealer.set_reveal_child(!needing.is_empty());
    }

    /// Render one workspace: tabs + panes + widgets + sidebar selection.
    pub fn show_workspace(&self, ws_id: &str) {
        let snapshot = self.model.borrow().cache.snapshots.get(ws_id).cloned();
        let Some(snapshot) = snapshot else { return };
        let ws = snapshot.workspace;
        self.title.set_title(&ws.name);
        let place = tilde(&ws.cwd);
        self.title.set_subtitle(&match &ws.git.branch {
            Some(branch) => format!("{place} · {branch}"),
            None => place,
        });
        let has_tabs = !snapshot.tabs.is_empty();
        {
            let mut m = self.model.borrow_mut();
            m.active_ws = Some(ws_id.to_string());
            m.tabs = snapshot.tabs;
            m.panes = snapshot
                .panes
                .into_iter()
                .map(|p| (p.id.clone(), p))
                .collect();
        }
        self.render_tabs();
        self.content
            .set_visible_child_name(if has_tabs { "tabs" } else { "no-tabs" });
        self.sidebar.select(ws_id);
    }

    // ---- layout rendering ----

    fn tab_id_of(&self, page: &adw::TabPage) -> Option<String> {
        self.tabs
            .borrow()
            .iter()
            .find(|(_, e)| &e.page == page)
            .map(|(id, _)| id.clone())
    }

    /// Reconcile tab pages with model tabs. A tab's widget tree is only
    /// rebuilt when its layout structure changed (split drags survive
    /// refreshes); ratio-only changes move the live dividers in place,
    /// and titles and status are updated in place.
    fn render_tabs(&self) {
        let (tabs, server_active) = {
            let m = self.model.borrow();
            let active = m
                .cache
                .workspaces
                .iter()
                .find(|w| Some(&w.id) == m.active_ws.as_ref())
                .and_then(|w| w.active_tab_id.clone());
            (m.tabs.clone(), active)
        };
        let ids: HashSet<String> = tabs.iter().map(|t| t.id.clone()).collect();
        // Decide selection first: appends and removals move it.
        let pick = self
            .gui_tab
            .borrow()
            .clone()
            .filter(|id| ids.contains(id))
            .or_else(|| server_active.filter(|id| ids.contains(id)))
            .or_else(|| tabs.first().map(|t| t.id.clone()));
        self.reconciling.set(true);

        let stale: Vec<(String, adw::TabPage)> = self
            .tabs
            .borrow()
            .iter()
            .filter(|(id, _)| !ids.contains(*id))
            .map(|(id, e)| (id.clone(), e.page.clone()))
            .collect();
        for (id, page) in stale {
            self.tabs.borrow_mut().remove(&id);
            self.drop_paneds(&id);
            self.tab_view.close_page(&page);
        }
        for tab in &tabs {
            let existing = self
                .tabs
                .borrow()
                .get(&tab.id)
                .map(|e| (e.page.clone(), e.bin.clone(), e.layout.clone()));
            let page = match existing {
                Some((page, bin, old_layout)) => {
                    if !same_layout(&old_layout, &tab.layout) {
                        self.set_tab_layout(&bin, tab);
                        if let Some(e) = self.tabs.borrow_mut().get_mut(&tab.id) {
                            e.layout = tab.layout.clone();
                        }
                    } else {
                        self.apply_server_ratios(&tab.id, tab.layout.as_ref());
                    }
                    page
                }
                None => {
                    let bin = adw::Bin::new();
                    bin.add_css_class("tab-page");
                    self.set_tab_layout(&bin, tab);
                    let page = self.tab_view.append(&bin);
                    self.tabs.borrow_mut().insert(
                        tab.id.clone(),
                        TabEntry {
                            page: page.clone(),
                            bin,
                            layout: tab.layout.clone(),
                        },
                    );
                    page
                }
            };
            self.decorate_page(&page, tab);
        }
        *self.gui_tab.borrow_mut() = pick.clone();
        if let Some(page) = pick.and_then(|id| self.tabs.borrow().get(&id).map(|e| e.page.clone()))
        {
            self.tab_view.set_selected_page(&page);
        }
        self.reconciling.set(false);

        // Pane meta + GC.
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

    /// Tab title + roll-up status: spinner while an agent works, an
    /// attention icon, and the tab bar's glow when it is off-screen.
    fn decorate_page(&self, page: &adw::TabPage, tab: &Tab) {
        let m = self.model.borrow();
        let panes: Vec<&Pane> = tab
            .layout
            .as_ref()
            .map(|l| l.panes())
            .unwrap_or_default()
            .iter()
            .filter_map(|id| m.panes.get(id))
            .collect();
        let attention = status::worst_attention(panes.iter().copied());
        page.set_title(&tab.title);
        page.set_loading(panes.iter().any(|p| p.lifecycle == Lifecycle::Working));
        page.set_indicator_icon(
            status::attention_icon(attention)
                .map(gio::ThemedIcon::new)
                .as_ref(),
        );
        page.set_indicator_tooltip(status::attention_tooltip(attention));
        page.set_needs_attention(attention.severity() >= Attention::InputRequired.severity());
    }

    /// Ratio-only server updates move live dividers instead of
    /// rebuilding, which would drop in-flight drags and loop echoes.
    fn apply_server_ratios(&self, tab_id: &str, layout: Option<&Layout>) {
        let Some(layout) = layout else { return };
        let mut places: Vec<(Vec<bool>, i32)> = Vec::new();
        for (path, ratio) in self.dividers.apply_server(tab_id, layout) {
            let key = (tab_id.to_string(), path.clone());
            let total = self
                .paned_widgets
                .borrow()
                .get(&key)
                .and_then(|w| w.upgrade())
                .map(|paned| match paned.orientation() {
                    gtk4::Orientation::Vertical => paned.height(),
                    _ => paned.width(),
                })
                .unwrap_or(0);
            if total > 100 {
                places.push((path, (total as f32 * ratio) as i32));
            }
        }
        for (path, pos) in places {
            self.place_paned(tab_id, &path, pos);
        }
    }

    fn set_tab_layout(&self, bin: &adw::Bin, tab: &Tab) {
        // Drop the old tree first so reused pane cards are free to move.
        self.drop_paneds(&tab.id);
        bin.set_child(None::<&gtk4::Widget>);
        let child: gtk4::Widget = match &tab.layout {
            None => {
                let page = adw::StatusPage::new();
                page.add_css_class("compact");
                page.set_title("Empty Tab");
                page.set_description(Some("Start a terminal to get going."));
                let btn = gtk4::Button::with_label("New Terminal");
                btn.add_css_class("pill");
                btn.add_css_class("suggested-action");
                btn.set_halign(gtk4::Align::Center);
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
            Some(layout) => self.build_layout(&tab.id, &mut Vec::new(), layout),
        };
        if matches!(tab.layout, Some(Layout::Split { .. })) {
            bin.add_css_class("split");
        } else {
            bin.remove_css_class("split");
        }
        bin.set_child(Some(&child));
    }

    fn build_layout(&self, tab_id: &str, path: &mut Vec<bool>, layout: &Layout) -> gtk4::Widget {
        match layout {
            Layout::Pane { pane_id } => {
                let root = self.widget_for(pane_id).root.clone();
                unparent(root.upcast_ref());
                root.upcast()
            }
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
                path.push(false);
                paned.set_start_child(Some(&self.build_layout(tab_id, path, first)));
                path.pop();
                path.push(true);
                paned.set_end_child(Some(&self.build_layout(tab_id, path, second)));
                path.pop();
                paned.set_shrink_start_child(false);
                paned.set_shrink_end_child(false);
                paned.set_wide_handle(true);
                // Drags report back through on_paned_position; the sizing
                // pass positions this once allocated (see sync_paneds).
                let w = self.weak();
                let watched_tab = tab_id.to_string();
                let watched_path = path.clone();
                paned.connect_position_notify(move |paned| {
                    if let Some(a) = w.upgrade() {
                        a.on_paned_position(&watched_tab, &watched_path, paned);
                    }
                });
                self.dividers.track(tab_id, path.clone(), *ratio);
                self.paned_widgets
                    .borrow_mut()
                    .insert((tab_id.to_string(), path.clone()), paned.downgrade());
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
                on_action: Box::new(move |id, action| {
                    if let Some(a) = w2.upgrade() {
                        a.on_pane_action(id, action);
                    }
                }),
            },
        );
        if let Some(p) = self.model.borrow().panes.get(pane_id) {
            widget.update_meta(p);
        }
        widget.set_focused(self.focused_pane.borrow().as_deref() == Some(pane_id));
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
                self.banner.set_revealed(false);
                self.toast("Reconnected");
                self.pending_refresh.borrow_mut().full = true;
                self.schedule_refresh();
            }
            UiEvent::Disconnected => {
                self.banner.set_revealed(true);
            }
            UiEvent::ServerEvent { name, payload, .. } => {
                crate::metrics::record("event", &name);
                self.maybe_notify(&name, &payload);
                self.pending_refresh.borrow_mut().on_event(
                    &self.model.borrow().cache,
                    &name,
                    &payload,
                );
                self.schedule_refresh();
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
        let prev = self.focused_pane.replace(Some(pane_id.to_string()));
        let widgets = self.widgets.borrow();
        if let Some(w) = prev.as_deref().and_then(|id| widgets.get(id)) {
            w.set_focused(false);
        }
        if let Some(w) = widgets.get(pane_id) {
            w.set_focused(true);
        }
    }

    fn on_pane_action(&self, pane_id: &str, action: PaneAction) {
        match action {
            PaneAction::SplitRight => self.split_pane(pane_id, SplitDir::Right),
            PaneAction::SplitDown => self.split_pane(pane_id, SplitDir::Down),
            PaneAction::Close => self.close_pane(pane_id),
            PaneAction::Resume => self.resume_pane(pane_id),
        }
    }

    // ---- focus navigation ----

    pub fn focus_pane(&self, pane_id: &str) {
        let pane: Option<Pane> = self
            .actor
            .call("pane.get", json!({"pane_id": pane_id}))
            .ok()
            .and_then(|v| serde_json::from_value(v["pane"].clone()).ok());
        let Some(pane) = pane else { return };
        // Notification clicks can beat the next refresh batch.
        let known = self
            .model
            .borrow()
            .cache
            .snapshots
            .get(&pane.workspace_id)
            .is_some_and(|s| s.panes.iter().any(|p| p.id == pane_id));
        if !known {
            self.refresh_workspace(&pane.workspace_id);
        }
        // Bind first: an if-condition borrow would live into the body.
        let same_ws = self.model.borrow().active_ws.as_deref() == Some(pane.workspace_id.as_str());
        if !same_ws {
            self.show_workspace(&pane.workspace_id);
        }
        *self.gui_tab.borrow_mut() = Some(pane.tab_id.clone());
        let page = self.tabs.borrow().get(&pane.tab_id).map(|e| e.page.clone());
        if let Some(page) = page {
            self.tab_view.set_selected_page(&page);
        }
        let widget = self.widgets.borrow().get(pane_id).cloned();
        if let Some(w) = widget {
            w.focus();
        }
    }

    /// Focus a pane created by the last action once its widget exists.
    fn focus_created(&self, result: &Value) {
        let Some(id) = result["pane"]["id"].as_str().map(str::to_string) else {
            return;
        };
        let w = self.weak();
        glib::idle_add_local_once(move || {
            if let Some(a) = w.upgrade() {
                a.focus_pane(&id);
            }
        });
    }

    fn focus_next_unread(&self) {
        match self.actor.call("focus.next_unread", json!({})) {
            Ok(v) => match v.get("pane_id").and_then(|p| p.as_str()) {
                Some(id) => self.focus_pane(id),
                None => self.toast("Nothing needs your attention"),
            },
            Err(e) => self.toast(&format!("Couldn't find the next pane — {e}")),
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

    /// Open the New Workspace dialog (Ctrl+Shift+N, the sidebar "+"
    /// and the empty state all land here). Creation itself happens in
    /// `create_workspace` once the user confirms.
    fn action_new_workspace(&self) {
        if self.new_ws_open.replace(true) {
            return;
        }
        let active_cwd = self.active_ws_cwd();
        let w = self.weak();
        let w2 = self.weak();
        crate::new_workspace::show_dialog(
            &self.window,
            active_cwd.as_deref(),
            move |req| {
                if let Some(a) = w.upgrade() {
                    a.create_workspace(&req.name, &req.cwd, &req.argv);
                }
            },
            move || {
                if let Some(a) = w2.upgrade() {
                    a.new_ws_open.set(false);
                }
            },
        );
    }

    fn active_ws_cwd(&self) -> Option<String> {
        let m = self.model.borrow();
        let id = m.active_ws.as_ref()?;
        m.cache
            .workspaces
            .iter()
            .find(|ws| &ws.id == id)
            .map(|ws| ws.cwd.clone())
    }

    /// `signaltty new` over IPC: create the workspace, spawn the first
    /// pane in it, show it and focus the new terminal.
    fn create_workspace(&self, name: &str, cwd: &str, argv: &[String]) {
        let ws_id = match self
            .actor
            .call("workspace.create", json!({"name": name, "cwd": cwd}))
        {
            Ok(v) => v["workspace"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            Err(e) => {
                self.toast(&format!("Couldn't create the workspace — {e}"));
                return;
            }
        };
        if ws_id.is_empty() {
            self.toast("Couldn't create the workspace");
            return;
        }
        // No active tab yet: pane.spawn auto-creates the "agents" tab.
        match self
            .actor
            .call("pane.spawn", json!({"workspace_id": ws_id, "argv": argv}))
        {
            Ok(v) => self.focus_created(&v),
            Err(e) => {
                self.toast(&format!("Couldn't start a terminal — {e}"));
                return;
            }
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
                self.toast(&format!("Couldn't open a tab — {e}"));
                return;
            }
        };
        *self.gui_tab.borrow_mut() = Some(tab_id.clone());
        self.spawn_shell_in_tab(&tab_id);
    }

    fn spawn_shell_in_tab(&self, tab_id: &str) {
        let Some(ws) = self.active_ws_id() else {
            return;
        };
        match self.actor.call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": tab_id, "argv": [user_shell()]}),
        ) {
            Ok(v) => self.focus_created(&v),
            Err(e) => self.toast(&format!("Couldn't start a terminal — {e}")),
        }
        self.refresh_workspace(&ws);
    }

    fn action_split(&self, dir: SplitDir) {
        match self.current_pane_id() {
            Some(id) => self.split_pane(&id, dir),
            None => self.toast("No pane to split"),
        }
    }

    fn split_pane(&self, pane_id: &str, dir: SplitDir) {
        let direction = match dir {
            SplitDir::Right => "right",
            SplitDir::Down => "down",
        };
        match self.actor.call(
            "pane.split",
            json!({"pane_id": pane_id, "direction": direction}),
        ) {
            Ok(v) => self.focus_created(&v),
            Err(e) => self.toast(&format!("Couldn't split the pane — {e}")),
        }
        self.refresh_pane_workspace(pane_id);
    }

    fn action_close_pane(&self) {
        match self.current_pane_id() {
            Some(id) => self.close_pane(&id),
            None => self.toast("No pane to close"),
        }
    }

    fn close_pane(&self, pane_id: &str) {
        if let Err(e) = self.actor.call("pane.close", json!({"pane_id": pane_id})) {
            self.toast(&format!("Couldn't close the pane — {e}"));
        }
        self.refresh_pane_workspace(pane_id);
    }

    fn resume_pane(&self, pane_id: &str) {
        if let Err(e) = self.actor.call("pane.resume", json!({"pane_id": pane_id})) {
            self.toast(&format!("Couldn't resume the session — {e}"));
        }
        self.refresh_pane_workspace(pane_id);
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("signaltty")
            .application_icon("dev.signaltty.gui")
            .version(env!("CARGO_PKG_VERSION"))
            .comments("A native workspace for parallel AI coding agents.")
            .build();
        about.present(Some(&self.window));
    }
}

/// Layout equality ignoring ratios (`None` equals only `None`).
fn same_layout(a: &Option<Layout>, b: &Option<Layout>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_structure(b),
        _ => false,
    }
}

/// Detach a reused pane card from wherever the previous layout put it.
fn unparent(widget: &gtk4::Widget) {
    let Some(parent) = widget.parent() else {
        return;
    };
    if let Some(paned) = parent.downcast_ref::<gtk4::Paned>() {
        if paned.start_child().as_ref() == Some(widget) {
            paned.set_start_child(None::<&gtk4::Widget>);
        } else {
            paned.set_end_child(None::<&gtk4::Widget>);
        }
    } else if let Some(bin) = parent.downcast_ref::<adw::Bin>() {
        bin.set_child(None::<&gtk4::Widget>);
    }
}
