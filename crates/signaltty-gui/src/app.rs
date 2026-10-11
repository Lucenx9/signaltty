//! App shell. Pure client of the server API: refetches on events,
//! renders state, forwards input. Closing this window never touches
//! running sessions.
//!
//! ```text
//! AdwOverlaySplitView
//! ├─ sidebar  AdwToolbarView: [+] / [Search commands] / Workspaces rows / [Worktrees] [Preferences]
//! └─ content  AdwToolbarView
//!    ├─ header  [sidebar] workspace · agent   [● 2] [Board] [Changes] [tab+] [menu]
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

use signaltty_core::{Attention, Layout, Lifecycle, Pane, SplitDir, Tab, Task, Workspace};

use crate::actions;
use crate::actor::{IpcHandle, UiEvent, UiTx};
use crate::board;
use crate::notif::Notifier;
use crate::refresh::{PendingRefresh, Snapshot, WorkspaceCache};
use crate::sidebar::{self, Sidebar};
use crate::status;
use crate::task_chip::{self, TaskChipView, TaskIndex};
use crate::terminal::{PaneAction, PaneCallbacks, PaneWidget};
use crate::util::tilde;

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

struct Model {
    cache: WorkspaceCache,
    tasks: TaskIndex,
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
/// Width of the drag strip on the sidebar's trailing edge.
const SIDEBAR_HANDLE_PX: i32 = 6;

fn sidebar_pointer_width(width: i32, direction: gtk4::TextDirection, x: f64) -> f64 {
    if direction == gtk4::TextDirection::Rtl {
        f64::from(width) - x
    } else {
        x
    }
}

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
    /// Right-hand split: workspace content, with the Changes panel
    /// docked at the end (overlaid on narrow windows).
    changes_split: adw::OverlaySplitView,
    changes: crate::changes::ChangesPanel,
    sidebar_overlay: gtk4::Overlay,
    title: gtk4::Label,
    title_context: gtk4::Label,
    title_mark: gtk4::Label,
    crumb_button: gtk4::MenuButton,
    details: Rc<crate::details::DetailsCard>,
    sidebar: Sidebar,
    tab_view: adw::TabView,
    content: gtk4::Stack,
    attention: AttentionButton,
    actor: IpcHandle,
    notifier: Notifier,
    model: RefCell<Model>,
    pending_refresh: RefCell<PendingRefresh>,
    refresh_scheduled: Cell<bool>,
    refresh_lock: tokio::sync::Mutex<()>,
    #[cfg(test)]
    pending_actions: Cell<usize>,
    widgets: RefCell<HashMap<String, Rc<PaneWidget>>>,
    tabs: RefCell<HashMap<String, TabEntry>>,
    /// Set while the GUI itself mutates the tab view, so selection
    /// changes it causes are not mistaken for the user's choice.
    reconciling: Cell<bool>,
    gui_tab: RefCell<Option<String>>,
    navigation: Cell<u64>,
    focused_pane: RefCell<Option<String>>,
    zoom: RefCell<Option<(String, String)>>,
    palette_dialog: RefCell<Option<adw::Dialog>>,
    board_dialog: RefCell<Option<board::Board>>,
    /// Divider state machine (ratios, drags, echo suppression);
    /// widgets live separately in `paned_widgets`.
    dividers: crate::dividers::Dividers,
    /// Weak divider widgets keyed like the state machine; App feeds
    /// observations from these and executes its commands on them.
    paned_widgets: RefCell<PanedWidgets>,
    /// The New Workspace dialog is single-instance: repeats of the
    /// action (or its accelerator) while it is open are ignored.
    new_ws_open: Cell<bool>,
    close_ws_dialog: RefCell<Option<adw::AlertDialog>>,
    pub preference: RefCell<signaltty_core::theme::GuiPreference>,
    me: RefCell<Weak<App>>,
}

pub(crate) fn user_shell() -> String {
    signaltty_core::paths::user_shell()
}

/// Toast text for a failed load, or `None` when the failure is the lost
/// connection itself: the banner already says the server is unreachable,
/// so a toast would repeat it (HIG: ongoing states belong in a banner,
/// toasts are for single events). The reply can be handled before the
/// `Disconnected` event reveals the banner, hence the error check too.
fn load_error_toast(banner_revealed: bool, what: &str, error: &str) -> Option<String> {
    let disconnected = banner_revealed || error == crate::actor::RECONNECTING;
    (!disconnected).then(|| format!("Couldn't load {what} — {error}"))
}

/// Map from pane_id -> workspace_id built from snapshot cache.
fn pane_to_workspace_map(snapshots: &HashMap<String, Snapshot>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for snapshot in snapshots.values() {
        for pane in &snapshot.panes {
            map.insert(pane.id.clone(), snapshot.workspace.id.clone());
        }
    }
    map
}

/// Derives parent workspace ID and associated Task for each child task workspace.
fn task_workspace_info(
    tasks: &TaskIndex,
    pane_to_ws: &HashMap<String, String>,
) -> (HashMap<String, String>, HashMap<String, Task>) {
    let mut parents = HashMap::new();
    let mut child_tasks = HashMap::new();
    for task in tasks.iter() {
        let (Some(pane_id), Some(parent_pane_id)) =
            (task.pane_id.as_deref(), task.parent_pane_id.as_deref())
        else {
            continue;
        };
        let (Some(child_ws), Some(parent_ws)) =
            (pane_to_ws.get(pane_id), pane_to_ws.get(parent_pane_id))
        else {
            continue;
        };
        if child_ws != parent_ws {
            parents.insert(child_ws.clone(), parent_ws.clone());
            child_tasks.insert(child_ws.clone(), task.clone());
        }
    }
    (parents, child_tasks)
}

fn resolve_root<'a>(mut curr: &'a str, parent_map: &'a HashMap<String, String>) -> &'a str {
    let mut visited = HashSet::new();
    while visited.insert(curr) {
        if let Some(p) = parent_map.get(curr) {
            curr = p.as_str();
        } else {
            break;
        }
    }
    curr
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
        let wordmark = gtk4::Label::new(Some("signaltty"));
        wordmark.add_css_class("wordmark");
        sidebar_header.set_title_widget(Some(&gtk4::Box::new(gtk4::Orientation::Horizontal, 0)));
        sidebar_header.pack_start(&wordmark);
        let btn_new_ws = gtk4::Button::from_icon_name("list-add-symbolic");
        btn_new_ws.set_tooltip_text(Some("New Workspace (Ctrl+Shift+N)"));
        btn_new_ws.update_property(&[gtk4::accessible::Property::Label("New Workspace")]);
        btn_new_ws.set_action_name(Some("win.new-workspace"));
        sidebar_header.pack_end(&btn_new_ws);
        // A visible entry point to the existing command palette. This is
        // deliberately a button, not an editable search field: typing happens
        // in the palette, which already supports keyboard navigation.
        let command_launcher = gtk4::Button::new();
        command_launcher.add_css_class("sidebar-command-launcher");
        command_launcher.set_action_name(Some("win.command-palette"));
        command_launcher.set_tooltip_text(Some("Search commands and workspaces (Ctrl+Shift+P)"));
        command_launcher.update_property(&[gtk4::accessible::Property::Label(
            "Search commands and workspaces",
        )]);
        command_launcher.set_child(Some(&crate::sidebar::labeled_content(
            "signaltty-search-symbolic",
            "Search or run a command…",
        )));
        let command_strip = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        command_strip.add_css_class("sidebar-command-strip");
        command_strip.append(&command_launcher);

        let sidebar_page = adw::ToolbarView::new();
        sidebar_page.add_top_bar(&sidebar_header);
        sidebar_page.add_top_bar(&command_strip);
        sidebar_page.set_content(Some(&sidebar.widget));
        // Pinned footer: tools that are not already in the content header
        // (Task Board and Changes live there with their toggle state), plus
        // settings. A real destination, unlike a mock status footer that
        // would incorrectly claim connectivity.
        let worktrees_button = crate::sidebar::nav_button(
            "signaltty-branch-symbolic",
            "Worktrees",
            "Manage Git Worktrees",
            "win.worktrees",
        );
        let footer_button = crate::sidebar::nav_button(
            "signaltty-settings-symbolic",
            "Preferences",
            "Preferences (Ctrl+,)",
            "win.preferences",
        );
        let sidebar_footer = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        sidebar_footer.add_css_class("sidebar-footer");
        sidebar_footer.append(&worktrees_button);
        sidebar_footer.append(&footer_button);
        sidebar_page.add_bottom_bar(&sidebar_footer);

        let sidebar_overlay = gtk4::Overlay::new();
        sidebar_overlay.set_child(Some(&sidebar_page));

        let sidebar_handle = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        sidebar_handle.add_css_class("sidebar-handle");
        sidebar_handle.set_halign(gtk4::Align::End);
        sidebar_handle.set_vexpand(true);
        sidebar_handle.set_width_request(SIDEBAR_HANDLE_PX);
        sidebar_handle.set_cursor_from_name(Some("col-resize"));
        sidebar_handle.set_focusable(true);
        sidebar_handle.update_property(&[gtk4::accessible::Property::Label("Resize Sidebar")]);
        sidebar_overlay.add_overlay(&sidebar_handle);

        // ---- content header ----
        // Left-aligned breadcrumb (Linear-style): mark · name · context.
        let title = gtk4::Label::new(Some("signaltty"));
        title.add_css_class("crumb-title");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let title_context = gtk4::Label::new(None);
        title_context.add_css_class("crumb-context");
        title_context.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        title_context.set_visible(false);
        let title_mark = sidebar::mark();
        title_mark.set_visible(false);
        let title_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        title_box.add_css_class("crumb");
        title_box.append(&title_mark);
        title_box.append(&title);
        let crumb_sep = gtk4::Label::new(Some("/"));
        crumb_sep.add_css_class("crumb-sep");
        title_box.append(&crumb_sep);
        title_context
            .bind_property("visible", &crumb_sep, "visible")
            .sync_create()
            .build();
        title_box.append(&title_context);
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&gtk4::Box::new(gtk4::Orientation::Horizontal, 0)));
        let btn_sidebar = gtk4::ToggleButton::new();
        btn_sidebar.set_icon_name("sidebar-show-symbolic");
        btn_sidebar.set_tooltip_text(Some("Toggle Sidebar (F9)"));
        btn_sidebar.update_property(&[gtk4::accessible::Property::Label("Toggle Sidebar")]);
        btn_sidebar.set_action_name(Some("win.toggle-sidebar"));
        header.pack_start(&btn_sidebar);
        // The breadcrumb opens the workspace details card.
        let details = crate::details::DetailsCard::new();
        let crumb_button = gtk4::MenuButton::new();
        crumb_button.set_child(Some(&title_box));
        crumb_button.set_popover(Some(&details.popover));
        crumb_button.add_css_class("crumb-button");
        crumb_button.set_tooltip_text(Some("Workspace Details"));
        // The name stays the visible title; the description says what it opens.
        crumb_button
            .update_property(&[gtk4::accessible::Property::Description("Workspace Details")]);
        crumb_button.set_sensitive(false);
        header.pack_start(&crumb_button);
        let btn_menu = gtk4::MenuButton::new();
        btn_menu.set_icon_name("open-menu-symbolic");
        btn_menu.set_tooltip_text(Some("Main Menu"));
        btn_menu.update_property(&[gtk4::accessible::Property::Label("Main Menu")]);
        btn_menu.set_menu_model(Some(&crate::actions::primary_menu()));
        btn_menu.set_primary(true);
        header.pack_end(&btn_menu);
        let btn_new_tab = gtk4::Button::from_icon_name("signaltty-tab-new-symbolic");
        btn_new_tab.set_tooltip_text(Some("New Tab (Ctrl+Shift+T)"));
        btn_new_tab.update_property(&[gtk4::accessible::Property::Label("New Tab")]);
        btn_new_tab.set_action_name(Some("win.new-tab"));
        header.pack_end(&btn_new_tab);
        // First-class tools have a label at desktop widths, and return to
        // icon-only controls where the workspace breadcrumb needs the room.
        let btn_changes = gtk4::ToggleButton::new();
        btn_changes.add_css_class("header-tool-button");
        btn_changes.set_tooltip_text(Some("Changes (Ctrl+Shift+D)"));
        btn_changes.update_property(&[gtk4::accessible::Property::Label("Changes")]);
        let changes_content = adw::ButtonContent::builder()
            .icon_name("sidebar-show-right-symbolic")
            .label("Changes")
            .build();
        btn_changes.set_child(Some(&changes_content));
        header.pack_end(&btn_changes);
        let btn_board = gtk4::Button::new();
        btn_board.add_css_class("header-tool-button");
        btn_board.set_tooltip_text(Some("Task Board (Ctrl+Shift+B)"));
        btn_board.update_property(&[gtk4::accessible::Property::Label("Task Board")]);
        btn_board.set_action_name(Some("win.show-board"));
        let board_content = adw::ButtonContent::builder()
            .icon_name("signaltty-board-symbolic")
            .label("Task Board")
            .build();
        btn_board.set_child(Some(&board_content));
        header.pack_end(&btn_board);
        let attention = Self::attention_button();
        header.pack_end(&attention.revealer);

        let banner = adw::Banner::new("Lost connection to the session server — retrying…");

        // ---- tabs + empty states ----
        let tab_view = adw::TabView::new();
        let tab_bar = adw::TabBar::new();
        tab_bar.set_view(Some(&tab_view));
        tab_bar.set_autohide(true);

        let no_workspace = adw::StatusPage::new();
        no_workspace.add_css_class("compact");
        no_workspace.set_icon_name(Some("utilities-terminal-symbolic"));
        no_workspace.set_title("Start with a project");
        no_workspace.set_description(Some(
            "Choose a folder, then start a shell or coding agent. \
             Your sessions keep running when this window closes.",
        ));
        let btn_empty = gtk4::Button::with_label("New Workspace");
        btn_empty.add_css_class("pill");
        btn_empty.add_css_class("suggested-action");
        btn_empty.set_halign(gtk4::Align::Center);
        btn_empty.set_action_name(Some("win.new-workspace"));
        let workspace_actions = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        workspace_actions.append(&btn_empty);
        let (key, modifiers) = gtk4::accelerator_parse("<Control><Shift>n").unwrap();
        let workspace_shortcut =
            gtk4::Label::new(Some(&gtk4::accelerator_get_label(key, modifiers)));
        workspace_shortcut.add_css_class("empty-state-shortcut");
        workspace_shortcut.set_halign(gtk4::Align::Center);
        workspace_actions.append(&workspace_shortcut);
        no_workspace.set_child(Some(&workspace_actions));

        let no_tabs = adw::StatusPage::new();
        no_tabs.add_css_class("compact");
        no_tabs.set_icon_name(Some("signaltty-tab-new-symbolic"));
        no_tabs.set_title("Open a terminal");
        no_tabs.set_description(Some("Start a shell."));
        let btn_no_tabs = gtk4::Button::with_label("New Tab");
        btn_no_tabs.add_css_class("pill");
        btn_no_tabs.add_css_class("suggested-action");
        btn_no_tabs.set_halign(gtk4::Align::Center);
        btn_no_tabs.set_action_name(Some("win.new-tab"));
        let tab_actions = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        tab_actions.append(&btn_no_tabs);
        let (key, modifiers) = gtk4::accelerator_parse("<Control><Shift>t").unwrap();
        let tab_shortcut = gtk4::Label::new(Some(&gtk4::accelerator_get_label(key, modifiers)));
        tab_shortcut.add_css_class("empty-state-shortcut");
        tab_shortcut.set_halign(gtk4::Align::Center);
        tab_actions.append(&tab_shortcut);
        no_tabs.set_child(Some(&tab_actions));

        let content = gtk4::Stack::new();
        content.set_transition_type(gtk4::StackTransitionType::None);
        content.add_named(&tab_view, Some("tabs"));
        content.add_named(&no_tabs, Some("no-tabs"));
        content.add_named(&no_workspace, Some("no-workspace"));

        let content_page = adw::ToolbarView::new();
        content_page.add_top_bar(&header);
        content_page.add_top_bar(&banner);
        content_page.add_top_bar(&tab_bar);
        content_page.set_content(Some(&content));

        let changes = crate::changes::ChangesPanel::new(actor.clone());
        let changes_split = adw::OverlaySplitView::new();
        changes_split.set_sidebar_position(gtk4::PackType::End);
        changes_split.set_sidebar_width_unit(adw::LengthUnit::Px);
        changes_split.set_min_sidebar_width(320.0);
        changes_split.set_max_sidebar_width(440.0);
        changes_split.set_sidebar_width_fraction(0.36);
        changes_split.set_show_sidebar(false);
        changes_split.set_sidebar(Some(&changes.widget));
        changes_split.set_content(Some(&content_page));
        changes_split
            .bind_property("show-sidebar", &btn_changes, "active")
            .bidirectional()
            .sync_create()
            .build();

        let split_view = adw::OverlaySplitView::new();
        split_view.set_sidebar_width_unit(adw::LengthUnit::Px);
        split_view.set_sidebar(Some(&sidebar_overlay));
        split_view.set_content(Some(&changes_split));
        split_view.set_min_sidebar_width(260.0);
        split_view.set_max_sidebar_width(340.0);
        split_view.set_sidebar_width_fraction(0.24);

        // Only the last matching breakpoint applies, so the wider one
        // goes first and the narrow one repeats its setters.
        // Below 1100sp the docked panel would squeeze the terminals: it
        // overlays them instead.
        let medium = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 1100sp").expect("breakpoint"),
        );
        medium.add_setter(&changes_split, "collapsed", Some(&true.to_value()));
        // Icon-only at this width: AdwButtonContent hides an empty label,
        // and the buttons drop the labeled pill for the same flat
        // `.image-button` look as New Tab and the main menu.
        medium.add_setter(&changes_content, "label", Some(&"".to_value()));
        medium.add_setter(&board_content, "label", Some(&"".to_value()));
        medium.add_setter(
            &btn_changes,
            "css-classes",
            Some(&["toggle", "image-button"].to_value()),
        );
        medium.add_setter(
            &btn_board,
            "css-classes",
            Some(&["image-button"].to_value()),
        );
        window.add_breakpoint(medium);
        // Narrow windows: the sidebar overlays instead of squeezing panes.
        let narrow = adw::Breakpoint::new(
            adw::BreakpointCondition::parse("max-width: 760sp").expect("breakpoint"),
        );
        narrow.add_setter(&split_view, "collapsed", Some(&true.to_value()));
        narrow.add_setter(&changes_split, "collapsed", Some(&true.to_value()));
        // At 360px the terminal and breadcrumb take precedence. Board and
        // Changes remain in the main menu and command palette, with their
        // existing shortcuts; their icons return at wider widths.
        narrow.add_setter(&btn_changes, "visible", Some(&false.to_value()));
        narrow.add_setter(&btn_board, "visible", Some(&false.to_value()));
        window.add_breakpoint(narrow);

        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split_view));
        window.set_content(Some(&toasts));

        let app = Rc::new(App {
            window,
            toasts,
            banner,
            split_view,
            changes_split,
            changes,
            sidebar_overlay,
            title,
            title_context,
            title_mark,
            crumb_button,
            details,
            sidebar,
            tab_view,
            content,
            attention,
            actor,
            notifier: Notifier::new(ui_tx),
            model: RefCell::new(Model {
                cache: WorkspaceCache::default(),
                tasks: TaskIndex::default(),
                active_ws: None,
                tabs: Vec::new(),
                panes: HashMap::new(),
            }),
            pending_refresh: RefCell::new(PendingRefresh::default()),
            refresh_scheduled: Cell::new(false),
            refresh_lock: tokio::sync::Mutex::new(()),
            #[cfg(test)]
            pending_actions: Cell::new(0),
            widgets: RefCell::new(HashMap::new()),
            tabs: RefCell::new(HashMap::new()),
            reconciling: Cell::new(false),
            gui_tab: RefCell::new(None),
            navigation: Cell::new(0),
            focused_pane: RefCell::new(None),
            zoom: RefCell::new(None),
            palette_dialog: RefCell::new(None),
            board_dialog: RefCell::new(None),
            dividers: crate::dividers::Dividers::new(),
            paned_widgets: RefCell::new(HashMap::new()),
            new_ws_open: Cell::new(false),
            close_ws_dialog: RefCell::new(None),
            preference: RefCell::new(crate::preferences::load_preference()),
            me: RefCell::new(Weak::new()),
        });
        app.me.replace(Rc::downgrade(&app));
        let keys = gtk4::EventControllerKey::new();
        let w = app.weak();
        keys.connect_key_pressed(move |_, key, _, _| {
            let delta = match key {
                gtk4::gdk::Key::Left => -10.0,
                gtk4::gdk::Key::Right => 10.0,
                _ => return glib::Propagation::Proceed,
            };
            let Some(a) = w.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let delta = if a.sidebar_overlay.direction() == gtk4::TextDirection::Rtl {
                -delta
            } else {
                delta
            };
            let width = a
                .preference()
                .sidebar_width
                .map(f64::from)
                .unwrap_or_else(|| f64::from(a.sidebar_overlay.width()));
            a.set_sidebar_width(signaltty_core::clamp_sidebar_width(width + delta));
            glib::Propagation::Stop
        });
        sidebar_handle.add_controller(keys);
        app.install_actions(application);
        app.connect_signals();
        app.apply_preference(app.preference());
        app
    }

    fn attention_button() -> AttentionButton {
        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        dot.set_valign(gtk4::Align::Center);
        let count = gtk4::Label::new(None);
        count.add_css_class("numeric");
        count.add_css_class("heading");
        let inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        inner.append(&dot);
        inner.append(&count);
        let button = gtk4::Button::new();
        button.set_child(Some(&inner));
        button.add_css_class("attention-button");
        button.set_valign(gtk4::Align::Center);
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
        let str_method = |f: fn(&App, &str)| {
            let w = self.weak();
            Box::new(move |param: String| {
                if let Some(a) = w.upgrade() {
                    f(&a, &param);
                }
            }) as Box<dyn Fn(String)>
        };

        crate::actions::install(
            &self.window,
            application,
            &self.split_view,
            crate::actions::ActionHandlers {
                palette: method(App::action_palette),
                rename_workspace: method(App::action_rename_workspace),
                search_terminal: method(App::action_search_terminal),
                zoom_pane: method(App::action_zoom_pane),
                worktrees: method(App::action_worktrees),
                show_changes: method(App::action_show_changes),
                show_board: method(App::action_show_board),
                new_workspace: method(App::action_new_workspace),
                close_workspace: method(App::action_close_active_workspace),
                new_tab: method(App::action_new_tab),
                split_right: split(SplitDir::Right),
                split_down: split(SplitDir::Down),
                close_pane: method(App::action_close_pane),
                next_attention: method(App::focus_next_unread),
                preferences: method(App::action_preferences),
                about: method(App::show_about),
            },
            crate::actions::TaskActionHandlers {
                task_open_pr: str_method(App::action_task_open_pr),
                task_create_pr: str_method(App::action_task_create_pr),
                task_merge: str_method(App::action_task_merge),
                task_cancel: str_method(App::action_task_cancel),
                task_discard: str_method(App::action_task_discard),
                clear_finished_tasks: str_method(App::action_clear_finished_tasks),
            },
        );
    }

    fn connect_signals(&self) {
        // Opening the details card fills it from the active workspace.
        let w = self.weak();
        self.details.popover.connect_show(move |_| {
            if let Some(a) = w.upgrade() {
                a.fill_details();
            }
        });
        // Opening the Changes panel loads the active workspace. Closing it
        // needs nothing: the split view hands focus back to the content.
        let w = self.weak();
        self.changes_split.connect_show_sidebar_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.sync_changes();
            }
        });
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
        let w = self.weak();
        self.sidebar.set_on_close(move |ws_id| {
            if let Some(a) = w.upgrade() {
                a.action_close_workspace(&ws_id);
            }
        });
        let w = self.weak();
        self.sidebar.set_menu_builder(move |ws_id| {
            if let Some(a) = w.upgrade() {
                a.build_sidebar_row_menu(ws_id)
            } else {
                gtk4::gio::Menu::new()
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
                    if a.gui_tab.borrow().as_deref() != Some(id.as_str()) {
                        a.navigate();
                    }
                    *a.gui_tab.borrow_mut() = Some(id);
                    if a.zoom
                        .borrow()
                        .as_ref()
                        .is_some_and(|(tab, _)| Some(tab) != a.gui_tab.borrow().as_ref())
                    {
                        a.zoom.borrow_mut().take();
                        a.render_tabs();
                    }
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
                a.run(move |app| async move {
                    if let Err(e) = app.actor.call("tab.close", json!({"tab_id": id})).await {
                        app.toast(&format!("Couldn't close the tab — {e}"));
                    }
                    app.refresh_later();
                });
            }
            glib::Propagation::Stop
        });
        // Terminals follow the desktop's colour scheme and mono font.
        let sm = adw::StyleManager::default();
        let w = self.weak();
        sm.connect_dark_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.restyle_terminals();
                a.sync_desktop_preferences();
            }
        });
        let w = self.weak();
        sm.connect_monospace_font_name_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.restyle_terminals();
            }
        });
        let w = self.weak();
        sm.connect_high_contrast_notify(move |_| {
            if let Some(a) = w.upgrade() {
                a.sync_desktop_preferences();
            }
        });
        let w = self.weak();
        self.window
            .settings()
            .connect_gtk_enable_animations_notify(move |_| {
                if let Some(a) = w.upgrade() {
                    a.sync_desktop_preferences();
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

        // Measure the pointer from the sidebar's fixed leading edge. In RTL,
        // GTK's local coordinates follow the moving left edge, so subtract
        // from the current overlay width to measure from the right edge.
        let drag = gtk4::GestureDrag::new();
        let overlay = self.sidebar_overlay.clone();
        drag.connect_drag_begin(move |g, x, _| {
            let width = sidebar_pointer_width(overlay.width(), overlay.direction(), x);
            if width < f64::from(overlay.width() - SIDEBAR_HANDLE_PX) {
                g.set_state(gtk4::EventSequenceState::Denied);
            }
        });
        let split = self.split_view.clone();
        let overlay = self.sidebar_overlay.clone();
        drag.connect_drag_update(move |g, dx, _| {
            let Some((x, _)) = g.start_point() else {
                return;
            };
            let width = sidebar_pointer_width(overlay.width(), overlay.direction(), x + dx);
            let w = f64::from(signaltty_core::clamp_sidebar_width(width));
            split.set_min_sidebar_width(w);
            split.set_max_sidebar_width(w);
        });
        let w = self.weak();
        drag.connect_drag_end(move |g, dx, _| {
            let (Some((x, _)), Some(a)) = (g.start_point(), w.upgrade()) else {
                return;
            };
            let width = sidebar_pointer_width(
                a.sidebar_overlay.width(),
                a.sidebar_overlay.direction(),
                x + dx,
            );
            a.set_sidebar_width(signaltty_core::clamp_sidebar_width(width));
        });
        self.sidebar_overlay.add_controller(drag);
    }

    fn restyle_terminals(&self) {
        let theme = self.preference.borrow().theme;
        for w in self.widgets.borrow().values() {
            w.apply_style(theme);
        }
    }

    fn sync_desktop_preferences(&self) {
        for (class, enabled) in [
            (
                "reduced-motion",
                !self.window.settings().is_gtk_enable_animations(),
            ),
            (
                "high-contrast",
                adw::StyleManager::default().is_high_contrast(),
            ),
            ("dark", adw::StyleManager::default().is_dark()),
        ] {
            if enabled {
                self.window.add_css_class(class);
            } else {
                self.window.remove_css_class(class);
            }
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
    /// sends complete asynchronously).
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
        let tab_id = tab_id.to_string();
        let path = path.to_vec();
        self.run(move |app| async move { app.send_ratio_async(&tab_id, &path, ratio).await });
    }

    async fn send_ratio_async(&self, tab_id: &str, path: &[bool], ratio: f32) {
        let ipath: Vec<u8> = path.iter().map(|b| u8::from(*b)).collect();
        match self
            .actor
            .call(
                "tab.set_ratio",
                json!({"tab_id": tab_id, "path": ipath, "ratio": ratio}),
            )
            .await
        {
            Ok(v) => {
                let confirmed = v
                    .get("tab")
                    .and_then(|t| t.get("layout"))
                    .and_then(|l| serde_json::from_value::<Layout>(l.clone()).ok())
                    .and_then(|l| l.ratio_at_path(path))
                    .unwrap_or(ratio);
                self.dividers.send_succeeded(tab_id, path, ratio, confirmed);
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
        if self
            .zoom
            .borrow()
            .as_ref()
            .is_some_and(|(tid, _)| tid == tab_id)
        {
            return;
        }
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

    fn run<F>(&self, task: impl FnOnce(Rc<App>) -> F + 'static)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        if let Some(app) = self.weak().upgrade() {
            #[cfg(test)]
            app.pending_actions.set(app.pending_actions.get() + 1);
            glib::spawn_future_local(async move {
                #[cfg(test)]
                let tracked = app.clone();
                task(app).await;
                #[cfg(test)]
                tracked
                    .pending_actions
                    .set(tracked.pending_actions.get() - 1);
            });
        }
    }

    fn navigate(&self) -> u64 {
        let next = self.navigation.get().wrapping_add(1);
        self.navigation.set(next);
        next
    }

    pub(crate) fn weak(&self) -> Weak<App> {
        self.me.borrow().clone()
    }

    fn action_preferences(&self) {
        let dialog = crate::preferences::build_dialog(self);
        dialog.present(Some(&self.window));
    }

    pub(crate) fn preference(&self) -> signaltty_core::theme::GuiPreference {
        *self.preference.borrow()
    }

    pub(crate) fn set_appearance(&self, appearance: signaltty_core::theme::Appearance) {
        let mut pref = *self.preference.borrow();
        if pref.appearance != appearance {
            pref.appearance = appearance;
            self.apply_preference(pref);
            crate::preferences::save_preference(&pref);
        }
    }

    pub(crate) fn set_theme(&self, theme: signaltty_core::theme::Theme) {
        let mut pref = *self.preference.borrow();
        if pref.theme != theme {
            pref.theme = theme;
            self.apply_preference(pref);
            crate::preferences::save_preference(&pref);
        }
    }

    pub(crate) fn set_sidebar_width(&self, width: u32) {
        let w = signaltty_core::clamp_sidebar_width(f64::from(width));
        self.split_view.set_min_sidebar_width(f64::from(w));
        self.split_view.set_max_sidebar_width(f64::from(w));
        let mut pref = self.preference.borrow_mut();
        pref.sidebar_width = Some(w);
        crate::preferences::save_preference(&pref);
    }

    pub(crate) fn apply_preference(&self, pref: signaltty_core::theme::GuiPreference) {
        *self.preference.borrow_mut() = pref;

        let scheme = match pref.appearance {
            signaltty_core::theme::Appearance::System => adw::ColorScheme::Default,
            signaltty_core::theme::Appearance::Light => adw::ColorScheme::ForceLight,
            signaltty_core::theme::Appearance::Dark => adw::ColorScheme::ForceDark,
        };
        adw::StyleManager::default().set_color_scheme(scheme);

        for t in signaltty_core::theme::Theme::ALL {
            self.window.remove_css_class(t.css_class());
        }
        self.window.add_css_class(pref.theme.css_class());

        // `parse` already clamped it; None keeps the 260–340 fraction default.
        if let Some(w) = pref.sidebar_width {
            self.split_view.set_min_sidebar_width(f64::from(w));
            self.split_view.set_max_sidebar_width(f64::from(w));
        }

        self.sync_desktop_preferences();
        self.restyle_terminals();
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
                a.run(move |app| async move {
                    app.apply_refresh(pending).await;
                    app.refresh_scheduled.set(false);
                    app.schedule_refresh();
                });
            }
        });
    }

    // ---- data ----

    /// Initial load, reconnect, or workspace creation/deletion only.
    pub fn refresh(&self) {
        self.pending_refresh.borrow_mut().full = true;
        self.schedule_refresh();
    }

    async fn refresh_workspace_async(&self, id: &str) {
        let mut pending = PendingRefresh::default();
        pending.workspaces.insert(id.to_string());
        self.apply_refresh(pending).await;
    }

    async fn refresh_pane_workspace_async(&self, pane_id: &str) {
        let mut pending = PendingRefresh::default();
        pending.on_event(
            &self.model.borrow().cache,
            "pane.updated",
            &json!({"pane_id": pane_id}),
        );
        self.apply_refresh(pending).await;
    }

    async fn refresh_async(&self) {
        self.apply_refresh(PendingRefresh::full()).await;
    }

    async fn apply_refresh(&self, pending: PendingRefresh) {
        let _serial = self.refresh_lock.lock().await;
        let full = pending.full;
        let mut cache = self.model.borrow().cache.clone();
        let (changed, errors) = cache
            .refresh(pending, |method, params| {
                let actor = self.actor.clone();
                async move { actor.call(method, params).await }
            })
            .await;
        self.model.borrow_mut().cache = cache;
        for error in errors {
            if let Some(msg) = load_error_toast(self.banner.is_revealed(), "workspaces", &error) {
                self.toast(&msg);
            }
        }
        if full {
            self.seed_tasks().await;
        }
        let active = {
            let mut m = self.model.borrow_mut();
            let keep = m
                .active_ws
                .clone()
                .filter(|id| m.cache.workspaces.iter().any(|ws| &ws.id == id));
            m.active_ws = keep.or_else(|| m.cache.workspaces.first().map(|ws| ws.id.clone()));
            m.active_ws.clone()
        };
        self.refresh_sidebar();
        match active {
            Some(id) if full || changed.contains(&id) => self.show_workspace_internal(&id, false),
            Some(_) => {}
            None => {
                {
                    let mut m = self.model.borrow_mut();
                    m.tabs.clear();
                    m.panes.clear();
                }
                self.render_tabs();
                self.title.set_text("signaltty");
                self.title_mark.set_visible(false);
                self.crumb_button.set_sensitive(false);
                self.details.popover.popdown();
                self.title_context.set_text("");
                self.title_context.set_visible(false);
                self.title_context.set_tooltip_text(None);
                self.content.set_visible_child_name("no-workspace");
                self.follow_changes();
            }
        }
        {
            let model = self.model.borrow();
            for pane in model
                .cache
                .snapshots
                .values()
                .flat_map(|snapshot| &snapshot.panes)
            {
                if let Some(widget) = self.widgets.borrow().get(&pane.id) {
                    widget.update_meta(pane);
                }
            }
        }
        self.prune_widgets(&self.collect_live_panes());
        self.paint_task_chips();
    }

    fn build_sidebar_items(&self) -> (Vec<sidebar::WsSummary>, Vec<Attention>) {
        let m = self.model.borrow();
        let pane_to_ws = pane_to_workspace_map(&m.cache.snapshots);
        let (parents, child_tasks) = task_workspace_info(&m.tasks, &pane_to_ws);

        let mut items = Vec::new();
        let mut needing = Vec::new();
        for ws in &m.cache.workspaces {
            if let Some(snapshot) = m.cache.snapshots.get(&ws.id) {
                let mut summary = sidebar::summarize(&snapshot.workspace, &snapshot.panes);
                summary.parent = parents.get(&ws.id).cloned();
                if let Some(task) = child_tasks.get(&ws.id) {
                    summary.finished = board::board_column(task) == board::BoardColumn::Done;
                }
                items.push(summary);
                needing.extend(
                    snapshot
                        .panes
                        .iter()
                        .filter(|p| p.attention.needs_human())
                        .map(|p| p.attention),
                );
            }
        }
        // Same-name workspaces are indistinguishable rows: show the
        // unique handle on each of them, nowhere else.
        let duplicated: HashSet<String> = {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for item in &items {
                *counts.entry(item.name.as_str()).or_default() += 1;
            }
            counts
                .into_iter()
                .filter(|(_, n)| *n > 1)
                .map(|(name, _)| name.to_string())
                .collect()
        };
        if !duplicated.is_empty() {
            for item in &mut items {
                if duplicated.contains(&item.name) {
                    item.disambiguator = m
                        .cache
                        .snapshots
                        .get(&item.id)
                        .map(|s| s.workspace.handle.clone())
                        .filter(|h| !h.is_empty());
                }
            }
        }
        (items, needing)
    }

    fn refresh_sidebar(&self) {
        let (mut items, needing) = self.build_sidebar_items();
        sidebar::sort_summaries(&mut items);
        self.sidebar.update(items);
        self.update_attention_button(&needing);
    }

    /// `task.list` on connect and reconnect. A list that races a newer
    /// `task.updated` must not put a cleared chip back.
    async fn seed_tasks(&self) {
        let ticket = self.model.borrow().tasks.seed_ticket();
        match self.actor.call("task.list", json!({"limit": 1000})).await {
            Ok(value) => {
                if let Some(tasks) = task_chip::parse_task_list(&value["tasks"]) {
                    self.model.borrow_mut().tasks.complete_seed(ticket, tasks);
                }
            }
            Err(e) => {
                if let Some(msg) = load_error_toast(self.banner.is_revealed(), "tasks", &e) {
                    self.toast(&msg);
                }
            }
        }
    }

    /// Header and sidebar chips from the pane-keyed cache. The header
    /// compares the task label with the pane title; the sidebar compares
    /// it with the workspace name. Widgets already on screen are updated;
    /// terminals are not rebuilt.
    fn paint_task_chips(&self) {
        let (by_pane, by_workspace) = {
            let model = self.model.borrow();
            let workspace_of = pane_to_workspace_map(&model.cache.snapshots);
            let mut parent_labels: HashMap<String, String> = HashMap::new();
            let mut pane_titles: HashMap<String, String> = HashMap::new();
            let mut workspace_names: HashMap<String, String> = HashMap::new();
            for snapshot in model.cache.snapshots.values() {
                workspace_names.insert(
                    snapshot.workspace.id.clone(),
                    snapshot.workspace.name.clone(),
                );
                for pane in &snapshot.panes {
                    let parent = pane
                        .label
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(pane.title.as_str());
                    parent_labels.insert(pane.id.clone(), parent.to_string());
                    pane_titles.insert(pane.id.clone(), pane.title.trim().to_string());
                }
            }
            let mut by_pane: HashMap<String, TaskChipView> = HashMap::new();
            let mut by_workspace: HashMap<String, Vec<TaskChipView>> = HashMap::new();
            for task in model.tasks.iter() {
                let Some(pane_id) = task.pane_id.as_deref() else {
                    continue;
                };
                let parent = task
                    .parent_pane_id
                    .as_deref()
                    .and_then(|id| parent_labels.get(id))
                    .map(String::as_str);
                let pane_name = pane_titles
                    .get(pane_id)
                    .map(String::as_str)
                    .filter(|name| !name.is_empty());
                by_pane.insert(
                    pane_id.to_string(),
                    task_chip::task_chip(task, parent, pane_name),
                );
                if let Some(ws) = workspace_of.get(pane_id) {
                    let ws_name = workspace_names
                        .get(ws)
                        .map(String::as_str)
                        .filter(|name| !name.is_empty());
                    by_workspace
                        .entry(ws.clone())
                        .or_default()
                        .push(task_chip::task_chip(task, parent, ws_name));
                }
            }
            (by_pane, by_workspace)
        };
        {
            let widgets = self.widgets.borrow();
            for (id, widget) in widgets.iter() {
                widget.set_task_chip(by_pane.get(id));
            }
        }
        self.sidebar.set_task_chips(&by_workspace);
    }

    fn update_attention_button(&self, needing: &[Attention]) {
        let b = &self.attention;
        let worst = needing.iter().fold(Attention::None, |acc, a| acc.raise(*a));
        b.count.set_text(&needing.len().to_string());
        b.button
            .update_property(&[gtk4::accessible::Property::Label(&format!(
                "Focus next pane needing attention: {}",
                needing.len()
            ))]);
        status::set_attention_class(&b.dot, worst);
        status::set_attention_class(&b.button, worst);
        b.button.set_tooltip_text(Some(&match needing.len() {
            1 => "1 pane needs attention — jump to it (Ctrl+Shift+J)".to_string(),
            n => format!("{n} panes need attention — jump to the next (Ctrl+Shift+J)"),
        }));
        b.revealer.set_reveal_child(!needing.is_empty());
    }

    /// The header title, disambiguated like the sidebar row when sibling
    /// workspaces share the name.
    fn display_title(&self, ws: &Workspace) -> String {
        let duplicated = self
            .model
            .borrow()
            .cache
            .workspaces
            .iter()
            .filter(|w| w.name == ws.name)
            .take(2)
            .count()
            > 1;
        if duplicated && !ws.handle.is_empty() {
            format!("{} · {}", ws.name, ws.handle)
        } else {
            ws.name.clone()
        }
    }

    /// Render one workspace: tabs + panes + widgets + sidebar selection.
    pub fn show_workspace(&self, ws_id: &str) {
        self.show_workspace_internal(ws_id, true);
    }

    fn show_workspace_internal(&self, ws_id: &str, user_navigation: bool) {
        let snapshot = self.model.borrow().cache.snapshots.get(ws_id).cloned();
        let Some(snapshot) = snapshot else { return };
        let ws = snapshot.workspace;
        if self.active_ws_id().as_deref() != Some(ws_id) {
            self.zoom.borrow_mut().take();
            // The card describes one workspace; a switch closes it.
            self.details.popover.popdown();
        }
        if user_navigation && self.active_ws_id().as_deref() != Some(ws_id) {
            self.navigate();
        }
        self.title.set_text(&self.display_title(&ws));
        self.follow_changes();
        sidebar::set_mark(&self.title_mark, &ws.id, &ws.name);
        self.title_mark.set_visible(true);
        self.crumb_button.set_sensitive(true);
        // Agent workspaces show their agents; shells show their location.
        let agents = sidebar::summarize(&ws, &snapshot.panes).agents;
        let place = tilde(&ws.cwd);
        let tooltip = match ws.git.branch.as_deref().filter(|b| !b.trim().is_empty()) {
            Some(branch) => format!("{branch} · {place}"),
            None => place,
        };
        let context = if agents.is_empty() { &tooltip } else { &agents };
        self.title_context.set_text(context);
        self.title_context.set_visible(!context.is_empty());
        self.title_context.set_tooltip_text(Some(&tooltip));
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
        // Bulk page replacement (workspace switch) must not animate:
        // closing the selected page slides to a neighbor, then selecting
        // the new page slides again — terminals smear through both (the
        // ghost glyphs on switch). Mutate + select while hidden so a single
        // paint shows the final state. Same-workspace refreshes (page set
        // unchanged) skip the wrap: nothing would animate anyway.
        let bulk =
            !stale.is_empty() || tabs.iter().any(|t| !self.tabs.borrow().contains_key(&t.id));
        if bulk {
            self.tab_view.set_visible(false);
        }
        for (id, page) in stale {
            self.tabs.borrow_mut().remove(&id);
            self.drop_paneds(&id);
            self.tab_view.close_page(&page);
        }
        if self.zoom.borrow().as_ref().is_some_and(|(tid, pid)| {
            !tabs
                .iter()
                .any(|t| &t.id == tid && t.layout.as_ref().is_some_and(|l| l.panes().contains(pid)))
        }) {
            self.zoom.borrow_mut().take();
        }
        for source_tab in &tabs {
            if let Some(layout) = &source_tab.layout {
                for pane in layout.panes() {
                    self.widget_for(&pane);
                }
            }
            let mut projected = source_tab.clone();
            if let Some((tid, pid)) = self.zoom.borrow().as_ref() {
                if tid == &source_tab.id {
                    projected.layout = Some(Layout::Pane {
                        pane_id: pid.clone(),
                    });
                }
            }
            let tab = &projected;
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
        if bulk {
            self.tab_view.set_visible(true);
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
        page.set_loading(
            panes
                .iter()
                .any(|p| status::effective_lifecycle(p) == Lifecycle::Working),
        );
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
        widget.apply_style(self.preference.borrow().theme);
        widget.set_focused(self.focused_pane.borrow().as_deref() == Some(pane_id));
        self.widgets
            .borrow_mut()
            .insert(pane_id.to_string(), Rc::clone(&widget));
        widget
    }

    fn collect_live_panes(&self) -> HashSet<String> {
        self.model
            .borrow()
            .cache
            .snapshots
            .values()
            .flat_map(|snapshot| snapshot.panes.iter().map(|pane| pane.id.clone()))
            .collect()
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
            UiEvent::PtySnapshot { pane_id, data } => {
                if let Some(w) = self.widgets.borrow().get(&pane_id) {
                    w.replace_screen(&data);
                }
            }
            UiEvent::PtyData { pane_id, data } => {
                if let Some(w) = self.widgets.borrow().get(&pane_id) {
                    w.feed(&data);
                }
            }
            UiEvent::FocusPane(id) => {
                self.window.present();
                self.focus_pane(&id);
            }
            UiEvent::MarkSeen(id) => {
                self.run(move |app| async move {
                    if app
                        .actor
                        .call("pane.mark_seen", json!({"pane_id": id}))
                        .await
                        .is_ok()
                    {
                        app.refresh_pane_workspace_async(&id).await;
                    }
                });
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
                let task_changed = self.model.borrow_mut().tasks.apply_event(&name, &payload);
                self.maybe_notify(&name, &payload);
                if self.changes_split.shows_sidebar() {
                    let workspace =
                        payload["workspace_id"]
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| {
                                let pane = payload["pane_id"].as_str()?;
                                self.model
                                    .borrow()
                                    .cache
                                    .pane_workspace(pane)
                                    .map(str::to_owned)
                            });
                    self.changes.on_event(&name, workspace.as_deref());
                }
                self.pending_refresh.borrow_mut().on_event(
                    &self.model.borrow().cache,
                    &name,
                    &payload,
                );
                if task_changed {
                    self.refresh_sidebar();
                    self.paint_task_chips();
                    self.refresh_open_board();
                }
                self.schedule_refresh();
            }
        }
    }

    /// Desktop notification for attention. Only on attention events:
    /// `notification.created` always precedes its attention raise, so
    /// notifying on both would double-notify.
    fn maybe_notify(&self, name: &str, payload: &Value) {
        let name = name.to_string();
        let payload = payload.clone();
        self.run(move |app| async move { app.maybe_notify_async(&name, &payload).await });
    }

    async fn maybe_notify_async(&self, name: &str, payload: &Value) {
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
        let (title, msg) = match self
            .actor
            .call("pane.get", json!({"pane_id": pane_id}))
            .await
        {
            Ok(v) => {
                let p = &v["pane"];
                (
                    p["title"].as_str().unwrap_or("signaltty").to_string(),
                    p["last_message"].as_str().unwrap_or(att).to_string(),
                )
            }
            Err(_) => ("signaltty".to_string(), att.to_string()),
        };
        if !self.is_pane_visible(pane_id) {
            self.notifier.notify_attention(&title, &msg, pane_id);
        }
    }

    fn is_pane_visible(&self, pane_id: &str) -> bool {
        self.focused_pane.borrow().as_deref() == Some(pane_id) && self.window.is_active()
    }

    fn on_pane_focused(&self, pane_id: &str) {
        let prev = self.focused_pane.replace(Some(pane_id.to_string()));
        if prev.as_deref() != Some(pane_id) {
            self.navigate();
        }
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
            PaneAction::AnswerDecision {
                decision_id,
                option_id,
            } => self.answer_decision(pane_id, &decision_id, &option_id),
        }
    }

    /// Deliver a decision pick; the bar clears on the resulting
    /// `decision.answered` event (or the toast explains a stale bar).
    fn answer_decision(&self, pane_id: &str, decision_id: &str, option_id: &str) {
        let pane_id = pane_id.to_string();
        let decision_id = decision_id.to_string();
        let option_id = option_id.to_string();
        self.run(move |app| async move {
            app.answer_decision_async(&pane_id, &decision_id, &option_id)
                .await
        });
    }

    async fn answer_decision_async(&self, pane_id: &str, decision_id: &str, option_id: &str) {
        if let Err(e) = self
            .actor
            .call(
                "decision.answer",
                json!({"pane_id": pane_id, "decision_id": decision_id, "option_id": option_id}),
            )
            .await
        {
            self.toast(&format!("Couldn't answer — {e}"));
        }
        self.refresh_pane_workspace_async(pane_id).await;
    }

    // ---- focus navigation ----

    pub fn focus_pane(&self, pane_id: &str) {
        let pane_id = pane_id.to_string();
        let navigation = self.navigate();
        self.run(move |app| async move { app.focus_pane_async(&pane_id, navigation).await });
    }

    async fn focus_pane_async(&self, pane_id: &str, navigation: u64) {
        let pane: Option<Pane> = self
            .actor
            .call("pane.get", json!({"pane_id": pane_id}))
            .await
            .ok()
            .and_then(|v| serde_json::from_value(v["pane"].clone()).ok());
        let Some(pane) = pane else { return };
        if self.navigation.get() != navigation {
            return;
        }
        // Notification clicks can beat the next refresh batch.
        let known = self
            .model
            .borrow()
            .cache
            .snapshots
            .get(&pane.workspace_id)
            .is_some_and(|s| s.panes.iter().any(|p| p.id == pane_id));
        if !known {
            self.refresh_workspace_async(&pane.workspace_id).await;
        }
        if self.navigation.get() != navigation {
            return;
        }
        // Bind first: an if-condition borrow would live into the body.
        let same_ws = self.model.borrow().active_ws.as_deref() == Some(pane.workspace_id.as_str());
        if !same_ws {
            self.show_workspace_internal(&pane.workspace_id, false);
        }
        if self
            .zoom
            .borrow()
            .as_ref()
            .is_some_and(|(tab, id)| tab != &pane.tab_id || id != pane_id)
        {
            self.zoom.borrow_mut().take();
            self.render_tabs();
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

    fn show_integration_notice(&self, result: &Value) {
        if let Some(notice) = result["integration"]["notice"].as_str() {
            let toast = adw::Toast::new(notice);
            toast.set_use_markup(false);
            toast.set_timeout(10);
            self.toasts.add_toast(toast);
        }
    }

    /// Focus a pane created by the last action once its widget exists.
    fn focus_created(&self, result: &Value, navigation: u64) {
        self.show_integration_notice(result);
        let Some(id) = result["pane"]["id"].as_str().map(str::to_string) else {
            return;
        };
        self.run(move |app| async move {
            if app.navigation.get() == navigation {
                app.focus_pane_async(&id, navigation).await;
            }
        });
    }

    fn focus_next_unread(&self) {
        let navigation = self.navigate();
        self.run(move |app| async move { app.focus_next_unread_async(navigation).await });
    }

    async fn focus_next_unread_async(&self, navigation: u64) {
        match self.actor.call("focus.next_unread", json!({})).await {
            Ok(v) => match v.get("pane_id").and_then(|p| p.as_str()) {
                Some(id) => self.focus_pane_async(id, navigation).await,
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
            .filter(|id| {
                m.panes
                    .get(id)
                    .is_some_and(|pane| self.gui_tab.borrow().as_ref() == Some(&pane.tab_id))
            })
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

    fn action_search_terminal(&self) {
        if let Some(id) = self.current_pane_id() {
            if let Some(widget) = self.widgets.borrow().get(&id) {
                widget.present_search();
            }
        }
    }

    fn action_zoom_pane(&self) {
        let Some(tab_id) = self.gui_tab.borrow().clone() else {
            return;
        };
        if self.zoom.borrow_mut().take().is_none() {
            let Some(pane) = self.current_pane_id() else {
                return;
            };
            let in_tab = self.model.borrow().tabs.iter().any(|t| {
                t.id == tab_id && t.layout.as_ref().is_some_and(|l| l.panes().contains(&pane))
            });
            if !in_tab {
                return;
            }
            *self.zoom.borrow_mut() = Some((tab_id, pane));
        }
        self.render_tabs();
        if let Some(id) = self.current_pane_id() {
            if let Some(widget) = self.widgets.borrow().get(&id) {
                widget.focus();
            }
        }
    }

    fn action_palette(&self) {
        if self.palette_dialog.borrow().is_some() {
            return;
        }
        let workspaces = self.model.borrow().cache.workspaces.clone();
        let weak = self.weak();
        let close_weak = self.weak();
        let dialog = crate::palette::present(
            &self.window,
            &workspaces,
            move |id| {
                if let Some(app) = weak.upgrade() {
                    app.show_workspace(id);
                    if let Some(pane) = app.current_pane_id() {
                        if let Some(widget) = app.widgets.borrow().get(&pane) {
                            widget.focus();
                        }
                    }
                }
            },
            move || {
                if let Some(app) = close_weak.upgrade() {
                    app.palette_dialog.borrow_mut().take();
                    if let Some(id) = app.current_pane_id() {
                        if let Some(w) = app.widgets.borrow().get(&id) {
                            w.focus();
                        }
                    }
                }
            },
        );
        *self.palette_dialog.borrow_mut() = Some(dialog);
    }

    fn action_rename_workspace(&self) {
        let Some(id) = self.active_ws_id() else {
            return;
        };
        let Some(ws) = self
            .model
            .borrow()
            .cache
            .workspaces
            .iter()
            .find(|w| w.id == id)
            .cloned()
        else {
            return;
        };
        crate::workspace_dialogs::rename(&self.window, self.actor.clone(), &ws);
    }

    fn action_worktrees(&self) {
        let Some(id) = self.active_ws_id() else {
            return;
        };
        let opened_at = self.navigation.get();
        let weak = self.weak();
        crate::workspace_dialogs::worktrees(&self.window, self.actor.clone(), &id, move |ws| {
            let Some(app) = weak.upgrade() else { return };
            if app.navigation.get() != opened_at {
                app.refresh_later();
                return;
            }
            let navigation = app.navigate();
            app.run(move |app| async move {
                let snapshot = app
                    .actor
                    .call("workspace.get", json!({"workspace_id": ws}))
                    .await;
                match snapshot {
                    Ok(snapshot) => {
                        if snapshot["panes"].as_array().is_some_and(|p| p.is_empty()) {
                            match app
                                .actor
                                .call(
                                    "pane.spawn",
                                    json!({"workspace_id": ws, "argv": [user_shell()]}),
                                )
                                .await
                            {
                                Ok(v) => app.focus_created(&v, navigation),
                                Err(e) => app
                                    .toast(&format!("Workspace opened; couldn't start shell: {e}")),
                            }
                        }
                        app.refresh_later();
                        app.refresh_async().await;
                        if app.navigation.get() == navigation {
                            app.show_workspace_internal(&ws, false);
                        }
                    }
                    Err(e) => app.toast(&e),
                }
            });
        });
    }

    fn action_show_changes(&self) {
        let open = !self.changes_split.shows_sidebar();
        self.changes_split.set_show_sidebar(open);
        if open {
            self.changes.focus();
        }
    }

    fn fill_details(&self) {
        let Some(id) = self.active_ws_id() else {
            return;
        };
        let snapshot = self.model.borrow().cache.snapshots.get(&id).cloned();
        let Some(snapshot) = snapshot else { return };
        let agents = sidebar::summarize(&snapshot.workspace, &snapshot.panes).agents;
        let details = crate::details::details(
            &snapshot.workspace,
            &snapshot.panes,
            &self.model.borrow().tasks,
            agents,
        );
        self.details.fill(details, &id, &self.actor);
    }

    /// Opening the panel reads the active workspace; a closed panel does
    /// no reads.
    fn sync_changes(&self) {
        if !self.changes_split.shows_sidebar() {
            return;
        }
        match self.active_ws_id() {
            Some(id) => self.changes.show(&id),
            None => self.changes.clear(),
        }
    }

    /// While open, the panel moves with the active workspace but does not
    /// re-read it: the model re-shows the workspace on every agent event.
    fn follow_changes(&self) {
        if !self.changes_split.shows_sidebar() {
            return;
        }
        match self.active_ws_id() {
            Some(id) => self.changes.follow(&id),
            None => self.changes.clear(),
        }
    }

    fn action_show_board(&self) {
        if self.board_dialog.borrow().is_some() {
            return;
        }
        self.present_board();
        self.run(move |app| async move {
            let _ = app.actor.call("task.pr_refresh", json!({})).await;
            app.seed_tasks().await;
            app.refresh_sidebar();
            app.paint_task_chips();
            app.refresh_open_board();
        });
    }

    fn refresh_open_board(&self) {
        if let Some(board) = self.board_dialog.borrow().as_ref() {
            let tasks = self
                .model
                .borrow()
                .tasks
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            board.update(&tasks);
        }
    }

    fn present_board(&self) {
        let tasks: Vec<signaltty_core::Task> = self.model.borrow().tasks.iter().cloned().collect();
        let weak = self.weak();
        let dialog = crate::board::present(&self.window, &tasks, move |closed, chosen| {
            let Some(app) = weak.upgrade() else { return };
            // Closing a replaced dialog must not clear its successor or move focus.
            if app
                .board_dialog
                .borrow()
                .as_ref()
                .map(|board| &board.dialog)
                != Some(closed)
            {
                return;
            }
            app.board_dialog.borrow_mut().take();
            if let Some(pane_id) = chosen {
                // After GTK hands focus back to the old pane, or that
                // focus-in counts as a newer navigation and wins.
                glib::idle_add_local_once(move || app.focus_pane(&pane_id));
            } else if let Some(id) = app.current_pane_id() {
                if let Some(w) = app.widgets.borrow().get(&id) {
                    w.focus();
                }
            }
        });
        *self.board_dialog.borrow_mut() = Some(dialog);
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

    /// Context menu for a sidebar row, built when it opens so task
    /// items match the task's current state.
    fn build_sidebar_row_menu(&self, ws_id: &str) -> gtk4::gio::Menu {
        let child_task = {
            let m = self.model.borrow();
            let pane_to_ws = pane_to_workspace_map(&m.cache.snapshots);
            task_workspace_info(&m.tasks, &pane_to_ws).1.remove(ws_id)
        };
        let kind = match child_task {
            Some(task) => actions::SidebarRowKind::Child(actions::TaskMenuFacts::from_task(&task)),
            None => actions::SidebarRowKind::Root {
                ws_id: ws_id.to_string(),
                finished_children_count: self.finished_children(ws_id).len(),
            },
        };
        actions::build_sidebar_menu(&kind)
    }

    /// Re-read tasks after a task action and repaint what shows them.
    async fn reload_tasks(&self) {
        self.seed_tasks().await;
        self.refresh_sidebar();
        self.paint_task_chips();
        self.refresh_open_board();
    }

    /// Run a task IPC call, then reload; failures surface as a toast.
    fn task_call(&self, method: &'static str, params: Value, failed: &'static str) {
        self.run(move |app| async move {
            match app.actor.call(method, params).await {
                Ok(_) => app.reload_tasks().await,
                Err(e) => app.toast(&format!("{failed} — {e}")),
            }
        });
    }

    /// Destructive confirmation shaped like `action_close_workspace`:
    /// Cancel is the default, `verb` runs `on_confirm`.
    fn confirm(&self, heading: &str, body: &str, verb: &str, on_confirm: impl Fn(&App) + 'static) {
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(body)
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("confirm", verb);
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let w = self.weak();
        dialog.connect_response(Some("confirm"), move |_, _| {
            if let Some(a) = w.upgrade() {
                on_confirm(&a);
            }
        });
        dialog.present(Some(&self.window));
    }

    fn task(&self, task_id: &str) -> Option<Task> {
        self.model.borrow().tasks.get_by_id(task_id).cloned()
    }

    fn action_task_open_pr(&self, task_id: &str) {
        let url = self
            .task(task_id)
            .and_then(|t| t.pr)
            .map(|pr| pr.url)
            .filter(|url| !url.is_empty());
        if let Some(url) = url {
            let weak = self.weak();
            gtk4::UriLauncher::new(&url).launch(
                Some(&self.window),
                gtk4::gio::Cancellable::NONE,
                move |result| {
                    if let Err(error) = result {
                        if let Some(app) = weak.upgrade() {
                            app.toast(&format!("Couldn't open pull request — {error}"));
                        }
                    }
                },
            );
        }
    }

    fn action_task_create_pr(&self, task_id: &str) {
        self.task_call(
            "task.pr_open",
            json!({"task_id": task_id}),
            "Couldn't create pull request",
        );
    }

    fn action_task_merge(&self, task_id: &str) {
        let Some(task) = self.task(task_id) else {
            return;
        };
        let target = task.target_branch.as_deref().unwrap_or("its target");
        let branch_fate = if task.preexisting_branch {
            format!("{} is kept", task.branch)
        } else {
            format!("{} is deleted", task.branch)
        };
        let params = json!({"task_id": task_id, "mode": "merge"});
        self.confirm(
            &format!("Merge {} into {target}?", task.label),
            &format!(
                "The worker stops and its worktree is removed; {branch_fate} after the merge."
            ),
            "Merge",
            move |a| a.task_call("task.finish", params.clone(), "Couldn't merge task"),
        );
    }

    fn action_task_cancel(&self, task_id: &str) {
        let Some(task) = self.task(task_id) else {
            return;
        };
        let params = json!({"task_id": task_id});
        self.confirm(
            &format!("Cancel {}?", task.label),
            "The worker stops. Its branch and worktree are kept.",
            "Cancel Task",
            move |a| a.task_call("task.cancel", params.clone(), "Couldn't cancel task"),
        );
    }

    fn action_task_discard(&self, task_id: &str) {
        let Some(task) = self.task(task_id) else {
            return;
        };
        let params = json!({"task_id": task_id, "mode": "discard"});
        self.confirm(
            &format!("Discard {}?", task.label),
            &format!(
                "The worker stops and its worktree is removed, with any uncommitted changes. {} is kept.",
                task.branch
            ),
            "Discard Task",
            move |a| a.task_call("task.finish", params.clone(), "Couldn't discard task"),
        );
    }

    /// Finished task workspaces nested under `root_ws_id`.
    fn finished_children(&self, root_ws_id: &str) -> Vec<String> {
        let m = self.model.borrow();
        let pane_to_ws = pane_to_workspace_map(&m.cache.snapshots);
        let (parents, child_tasks) = task_workspace_info(&m.tasks, &pane_to_ws);
        parents
            .keys()
            .filter(|child| {
                resolve_root(child, &parents) == root_ws_id
                    && child_tasks
                        .get(*child)
                        .is_some_and(|t| board::board_column(t) == board::BoardColumn::Done)
            })
            .cloned()
            .collect()
    }

    fn action_clear_finished_tasks(&self, root_ws_id: &str) {
        let ids = self.finished_children(root_ws_id);
        let heading = match ids.len() {
            0 => return,
            1 => "Clear 1 finished task?".to_string(),
            n => format!("Clear {n} finished tasks?"),
        };
        self.confirm(
            &heading,
            "Their workspaces close. Task branches and any remaining worktrees are kept.",
            "Clear",
            move |a| {
                let ids = ids.clone();
                a.run(move |app| async move {
                    for id in ids {
                        if let Err(e) = app
                            .actor
                            .call("workspace.close", json!({"workspace_id": id}))
                            .await
                        {
                            app.toast(&format!("Couldn't close a task workspace — {e}"));
                        }
                    }
                    app.refresh_async().await;
                });
            },
        );
    }

    fn action_close_active_workspace(&self) {
        if let Some(id) = self.active_ws_id() {
            self.action_close_workspace(&id);
        }
    }

    fn action_close_workspace(&self, id: &str) {
        if self.close_ws_dialog.borrow().is_some() {
            return;
        }
        let name = {
            let model = self.model.borrow();
            let Some(ws) = model.cache.workspaces.iter().find(|ws| ws.id == id) else {
                return;
            };
            ws.name.clone()
        };
        let dialog = adw::AlertDialog::builder()
            .heading(format!("Close {name}?"))
            .body("Running terminals and agents in this workspace will stop.")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("close", "Close Workspace");
        dialog.set_response_appearance("close", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let id = id.to_string();
        let w = self.weak();
        dialog.connect_response(Some("close"), move |_, _| {
            if let Some(a) = w.upgrade() {
                a.close_workspace(&id);
            }
        });
        let w = self.weak();
        dialog.connect_closed(move |_| {
            if let Some(a) = w.upgrade() {
                a.close_ws_dialog.borrow_mut().take();
            }
        });
        self.close_ws_dialog.replace(Some(dialog.clone()));
        dialog.present(Some(&self.window));
    }

    fn close_workspace(&self, id: &str) {
        let id = id.to_string();
        self.run(move |app| async move { app.close_workspace_async(&id).await });
    }

    async fn close_workspace_async(&self, id: &str) {
        match self
            .actor
            .call("workspace.close", json!({"workspace_id": id}))
            .await
        {
            Ok(_) => self.refresh_async().await,
            Err(e) => self.toast(&format!("Couldn't close the workspace — {e}")),
        }
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
        let name = name.to_string();
        let cwd = cwd.to_string();
        let argv = argv.to_vec();
        let navigation = self.navigate();
        self.run(move |app| async move {
            app.create_workspace_async(&name, &cwd, &argv, navigation)
                .await
        });
    }

    async fn create_workspace_async(
        &self,
        name: &str,
        cwd: &str,
        argv: &[String],
        navigation: u64,
    ) {
        let ws_id = match self
            .actor
            .call("workspace.create", json!({"name": name, "cwd": cwd}))
            .await
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
            .await
        {
            Ok(v) => self.focus_created(&v, navigation),
            Err(e) => {
                self.toast(&format!("Couldn't start a terminal — {e}"));
                return;
            }
        }
        self.refresh_async().await;
        if self.navigation.get() == navigation {
            self.show_workspace_internal(&ws_id, false);
        }
    }

    fn action_new_tab(&self) {
        let Some(ws) = self.active_ws_id() else {
            self.action_new_workspace();
            return;
        };
        let navigation = self.navigate();
        self.run(move |app| async move { app.action_new_tab_async(&ws, navigation).await });
    }

    async fn action_new_tab_async(&self, ws: &str, navigation: u64) {
        let tab_id = match self
            .actor
            .call("tab.create", json!({"workspace_id": ws, "title": "shell"}))
            .await
        {
            Ok(v) => v["tab"]["id"].as_str().unwrap_or_default().to_string(),
            Err(e) => {
                self.toast(&format!("Couldn't open a tab — {e}"));
                return;
            }
        };
        if self.navigation.get() == navigation {
            *self.gui_tab.borrow_mut() = Some(tab_id.clone());
        }
        self.spawn_shell_in_tab_async(ws, &tab_id, navigation).await;
    }

    fn spawn_shell_in_tab(&self, tab_id: &str) {
        let tab_id = tab_id.to_string();
        let Some(ws) = self.active_ws_id() else {
            return;
        };
        let navigation = self.navigate();
        self.run(
            move |app| async move { app.spawn_shell_in_tab_async(&ws, &tab_id, navigation).await },
        );
    }

    async fn spawn_shell_in_tab_async(&self, ws: &str, tab_id: &str, navigation: u64) {
        match self
            .actor
            .call(
                "pane.spawn",
                json!({"workspace_id": ws, "tab_id": tab_id, "argv": [user_shell()]}),
            )
            .await
        {
            Ok(v) => self.focus_created(&v, navigation),
            Err(e) => self.toast(&format!("Couldn't start a terminal — {e}")),
        }
        self.refresh_workspace_async(ws).await;
    }

    fn action_split(&self, dir: SplitDir) {
        match self.current_pane_id() {
            Some(id) => self.split_pane(&id, dir),
            None => self.toast("No pane to split"),
        }
    }

    fn split_pane(&self, pane_id: &str, dir: SplitDir) {
        let pane_id = pane_id.to_string();
        let navigation = self.navigate();
        self.run(move |app| async move { app.split_pane_async(&pane_id, dir, navigation).await });
    }

    async fn split_pane_async(&self, pane_id: &str, dir: SplitDir, navigation: u64) {
        let direction = match dir {
            SplitDir::Right => "right",
            SplitDir::Down => "down",
        };
        match self
            .actor
            .call(
                "pane.split",
                json!({"pane_id": pane_id, "direction": direction}),
            )
            .await
        {
            Ok(v) => self.focus_created(&v, navigation),
            Err(e) => self.toast(&format!("Couldn't split the pane — {e}")),
        }
        self.refresh_pane_workspace_async(pane_id).await;
    }

    fn action_close_pane(&self) {
        match self.current_pane_id() {
            Some(id) => self.close_pane(&id),
            None => self.toast("No pane to close"),
        }
    }

    fn close_pane(&self, pane_id: &str) {
        let pane_id = pane_id.to_string();
        self.run(move |app| async move { app.close_pane_async(&pane_id).await });
    }

    async fn close_pane_async(&self, pane_id: &str) {
        if let Err(e) = self
            .actor
            .call("pane.close", json!({"pane_id": pane_id}))
            .await
        {
            self.toast(&format!("Couldn't close the pane — {e}"));
        }
        self.refresh_pane_workspace_async(pane_id).await;
    }

    fn resume_pane(&self, pane_id: &str) {
        let pane_id = pane_id.to_string();
        self.run(move |app| async move { app.resume_pane_async(&pane_id).await });
    }

    async fn resume_pane_async(&self, pane_id: &str) {
        match self
            .actor
            .call("pane.resume", json!({"pane_id": pane_id}))
            .await
        {
            Ok(result) => self.show_integration_notice(&result),
            Err(e) => self.toast(&format!("Couldn't resume the session — {e}")),
        }
        self.refresh_pane_workspace_async(pane_id).await;
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
