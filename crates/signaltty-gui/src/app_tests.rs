use super::*;
use crate::actor::ActorRequest;
use crate::refresh::tests::fixture;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[test]
fn sidebar_pointer_tracks_the_trailing_edge_in_both_directions() {
    use gtk4::TextDirection::{Ltr, Rtl};
    // The six-pixel handle occupies opposite edges of the same allocation.
    assert_eq!(sidebar_pointer_width(380, Ltr, 377.0), 377.0);
    assert_eq!(sidebar_pointer_width(380, Rtl, 3.0), 377.0);
    assert_eq!(sidebar_pointer_width(380, Ltr, 3.0), 3.0);
    assert_eq!(sidebar_pointer_width(380, Rtl, 377.0), 3.0);
    // Drag past either edge; keep measuring from the fixed leading edge
    // even after allocation catches up and changes RTL local coordinates.
    assert_eq!(sidebar_pointer_width(380, Ltr, 417.0), 417.0);
    assert_eq!(sidebar_pointer_width(380, Rtl, -37.0), 417.0);
    assert_eq!(sidebar_pointer_width(417, Rtl, 0.0), 417.0);
    assert_eq!(sidebar_pointer_width(417, Rtl, 20.0), 397.0);
}

#[test]
fn load_errors_skip_the_toast_while_the_connection_is_down() {
    assert_eq!(
        load_error_toast(false, "workspaces", "workspace.list failed").as_deref(),
        Some("Couldn't load workspaces — workspace.list failed")
    );
    assert_eq!(
        load_error_toast(false, "tasks", "boom").as_deref(),
        Some("Couldn't load tasks — boom")
    );
    assert_eq!(load_error_toast(true, "workspaces", "reconnecting"), None);
    assert_eq!(load_error_toast(true, "tasks", "boom"), None);
    // The reply may beat the Disconnected event that reveals the banner.
    assert_eq!(
        load_error_toast(false, "tasks", crate::actor::RECONNECTING),
        None
    );
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn desktop_motion_preference_applies_at_startup_and_changes_live() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let original = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    assert!(app.window.has_css_class("reduced-motion"));
    settings.set_gtk_enable_animations(true);
    assert!(!app.window.has_css_class("reduced-motion"));
    settings.set_gtk_enable_animations(false);
    assert!(app.window.has_css_class("reduced-motion"));
    app.window.destroy();
    settings.set_gtk_enable_animations(original);
}

fn emit(app: &App, name: &str, payload: Value) {
    app.on_event(UiEvent::ServerEvent {
        name: name.into(),
        payload,
    });
}

fn drain_refresh(app: &App) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.refresh_scheduled.get() || app.pending_actions.get() > 0 {
        assert!(
            Instant::now() < deadline,
            "refresh starved on GTK main loop"
        );
        glib::MainContext::default().iteration(true);
    }
}

fn has_label(widget: &gtk4::Widget, text: &str) -> bool {
    if widget
        .downcast_ref::<gtk4::Label>()
        .is_some_and(|l| l.text() == text)
    {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(w) = child {
        if has_label(&w, text) {
            return true;
        }
        child = w.next_sibling();
    }
    false
}

// Verify the visible stage counter belongs to its heading rather than
// accidentally matching an unrelated task's numeric label.
fn has_board_column_count(widget: &gtk4::Widget, title: &str, count: &str) -> bool {
    if widget.has_css_class("board-column-heading") {
        if let Some(heading) = widget.first_child() {
            if heading
                .downcast_ref::<gtk4::Label>()
                .is_some_and(|label| label.text() == title)
            {
                return heading
                    .next_sibling()
                    .and_then(|w| w.downcast::<gtk4::Label>().ok())
                    .is_some_and(|label| {
                        label.has_css_class("board-column-count") && label.text() == count
                    });
            }
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if has_board_column_count(&current, title, count) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

fn button_with_tooltip(widget: &gtk4::Widget, tooltip: &str) -> Option<gtk4::Button> {
    if let Some(button) = widget.downcast_ref::<gtk4::Button>() {
        if button.tooltip_text().as_deref() == Some(tooltip) {
            return Some(button.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(widget) = child {
        if let Some(button) = button_with_tooltip(&widget, tooltip) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

fn respond(app: &App, dialog: &adw::AlertDialog, response: &str) {
    dialog.emit_by_name_with_details::<()>(
        "response",
        glib::Quark::from_str(response),
        &[&response],
    );
    dialog.close();
    drain_refresh(app);
    while glib::MainContext::default().iteration(false) {}
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session (or xvfb-run)"]
fn close_workspace_confirms_the_selected_target_and_refreshes() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let state = Arc::new(Mutex::new(BTreeMap::from([
        ("a".to_string(), fixture("a")),
        ("b".to_string(), fixture("b")),
    ])));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = state.clone();
    let recorded = calls.clone();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            match request {
                ActorRequest::Call {
                    method,
                    params,
                    reply,
                    ..
                } => {
                    if method == "test.stop" {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    if method == "pane.resize" {
                        let _ = reply.send(Ok(json!({})));
                        continue;
                    }
                    recorded
                        .lock()
                        .unwrap()
                        .push((method.clone(), params.clone()));
                    let mut state = server.lock().unwrap();
                    let result = match method.as_str() {
                        "workspace.list" => Ok(json!({"workspaces": state.values()
                            .map(|s| s["workspace"].clone()).collect::<Vec<_>>()})),
                        "workspace.get" => state
                            .get(params["workspace_id"].as_str().unwrap())
                            .cloned()
                            .ok_or("NO_SUCH_WORKSPACE".into()),
                        "workspace.close" => state
                            .remove(params["workspace_id"].as_str().unwrap())
                            .map(|_| json!({"closed": true}))
                            .ok_or("NO_SUCH_WORKSPACE".into()),
                        "task.list" => Ok(json!({"tasks": []})),
                        _ => panic!("unexpected IPC {method}"),
                    };
                    let _ = reply.send(result);
                }
                ActorRequest::Attach { .. } => {}
                ActorRequest::Detach { .. } => {}
            }
        }
    });
    let (ui, _events) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.window.present();
    app.refresh();
    drain_refresh(&app);
    assert_sidebar(&app, &["a", "b"], "a");
    calls.lock().unwrap().clear();

    let close_b = button_with_tooltip(app.sidebar.widget.upcast_ref(), "Close b")
        .expect("inactive workspace has a close button");
    close_b.emit_clicked();
    let dialog = app.close_ws_dialog.borrow().clone().expect("confirmation");
    assert!(dialog.heading().unwrap().contains('b'));
    assert_eq!(app.active_ws_id().as_deref(), Some("a"));
    respond(&app, &dialog, "cancel");
    assert!(calls.lock().unwrap().is_empty());
    assert_sidebar(&app, &["a", "b"], "a");

    // A remote close can race with this confirmation. The failed call
    // must leave the current sidebar intact.
    state.lock().unwrap().remove("b");
    close_b.emit_clicked();
    let dialog = app.close_ws_dialog.borrow().clone().expect("confirmation");
    respond(&app, &dialog, "close");
    assert_eq!(
        calls.lock().unwrap()[0],
        ("workspace.close".into(), json!({"workspace_id": "b"}))
    );
    assert_sidebar(&app, &["a", "b"], "a");
    state.lock().unwrap().insert("b".into(), fixture("b"));
    calls.lock().unwrap().clear();

    close_b.emit_clicked();
    let dialog = app.close_ws_dialog.borrow().clone().expect("confirmation");
    close_b.emit_clicked();
    assert_eq!(app.close_ws_dialog.borrow().as_ref(), Some(&dialog));
    respond(&app, &dialog, "close");
    assert_eq!(
        calls.lock().unwrap()[0],
        ("workspace.close".into(), json!({"workspace_id": "b"}))
    );
    assert_sidebar(&app, &["a"], "a");
    assert!(!state.lock().unwrap().contains_key("b"));
    calls.lock().unwrap().clear();

    app.window
        .lookup_action("close-workspace")
        .unwrap()
        .activate(None);
    let dialog = app
        .close_ws_dialog
        .borrow()
        .clone()
        .expect("menu confirmation");
    respond(&app, &dialog, "close");
    assert_eq!(
        calls.lock().unwrap()[0],
        ("workspace.close".into(), json!({"workspace_id": "a"}))
    );
    assert!(state.lock().unwrap().is_empty());
    assert!(app.active_ws_id().is_none());
    assert_eq!(
        app.content.visible_child_name().as_deref(),
        Some("no-workspace")
    );

    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    drop(app);
    worker.join().unwrap();
}

/// Sidebar rows in rendered order: (workspace id, selected).
fn sidebar_order(app: &App) -> Vec<(String, bool)> {
    fn walk(widget: &gtk4::Widget, out: &mut Vec<(String, bool)>) {
        if let Some(row) = widget.downcast_ref::<gtk4::ListBoxRow>() {
            out.push((row.widget_name().to_string(), row.is_selected()));
        }
        let mut child = widget.first_child();
        while let Some(w) = child {
            walk(&w, out);
            child = w.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(app.sidebar.widget.upcast_ref(), &mut out);
    out
}

fn assert_sidebar(app: &App, ids: &[&str], selected: &str) {
    let order = sidebar_order(app);
    let names: Vec<&str> = order.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(names, ids, "priority order");
    let sel: Vec<&str> = order
        .iter()
        .filter(|(_, s)| *s)
        .map(|(id, _)| id.as_str())
        .collect();
    assert_eq!(sel, [selected], "selection follows the workspace");
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session (or xvfb-run)"]
fn event_batches_keep_sidebar_attention_tabs_and_notifications_consistent() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let state = Arc::new(Mutex::new(BTreeMap::from([
        ("a".to_string(), fixture("a")),
        ("b".to_string(), fixture("b")),
    ])));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server = state.clone();
    let recorded = calls.clone();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            match request {
                ActorRequest::Call {
                    method,
                    params,
                    reply,
                    ..
                } => {
                    if method == "test.stop" {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    recorded
                        .lock()
                        .unwrap()
                        .push((method.clone(), params.clone()));
                    let state = server.lock().unwrap();
                    let result = match method.as_str() {
                        "workspace.list" => Ok(json!({"workspaces": state.values()
                            .map(|s| s["workspace"].clone()).collect::<Vec<_>>()})),
                        "workspace.get" => state
                            .get(params["workspace_id"].as_str().unwrap())
                            .cloned()
                            .ok_or("NO_SUCH_WORKSPACE".into()),
                        "pane.get" => state
                            .values()
                            .flat_map(|s| s["panes"].as_array().unwrap())
                            .find(|p| p["id"] == params["pane_id"])
                            .map(|p| json!({"pane": p}))
                            .ok_or("NO_SUCH_PANE".into()),
                        "task.list" => Ok(json!({"tasks": []})),
                        _ => panic!("unexpected IPC {method}"),
                    };
                    let _ = reply.send(result);
                }
                ActorRequest::Attach { .. } => {}
                ActorRequest::Detach { .. } => {}
            }
        }
    });
    let (ui, _events) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    drain_refresh(&app);
    assert_eq!(
        calls.lock().unwrap().len(),
        4,
        "one list + one get per workspace, plus task.list"
    );
    assert_eq!(app.active_ws_id().as_deref(), Some("a"));
    assert_sidebar(&app, &["a", "b"], "a");
    let original_page = app.tab_view.selected_page().unwrap();
    let original_terminal = app.widgets.borrow()["pane_a"].root.clone();
    calls.lock().unwrap().clear();

    // The inactive workspace changes while the active tab stays untouched.
    {
        let mut state = state.lock().unwrap();
        let b = state.get_mut("b").unwrap();
        b["panes"][0]["attention"] = json!("permission_required");
        b["panes"][0]["last_message"] = json!("approval in background");
        b["panes"][0]["lifecycle"] = json!("working");
        b["tabs"][0]["title"] = json!("updated tab");
        let mut tab = b["tabs"][0].clone();
        tab["id"] = json!("tab_b2");
        tab["title"] = json!("second tab");
        tab["layout"] = Value::Null;
        tab["active_pane_id"] = Value::Null;
        b["tabs"].as_array_mut().unwrap().push(tab);
        b["workspace"]["tabs"] = json!(["tab_b", "tab_b2"]);
    }
    for _ in 0..200 {
        emit(
            &app,
            "notification.created",
            json!({"notification": {"pane_id": "pane_b", "workspace_id": "b"}}),
        );
    }
    assert!(
        calls.lock().unwrap().is_empty(),
        "on_event must not refresh synchronously"
    );
    drain_refresh(&app);
    assert_eq!(
        *calls.lock().unwrap(),
        vec![("workspace.get".into(), json!({"workspace_id": "b"}))]
    );
    assert_eq!(app.attention.count.text(), "1");
    assert!(has_label(
        app.sidebar.widget.upcast_ref(),
        "approval in background"
    ));
    assert_eq!(app.tab_view.selected_page(), Some(original_page));
    assert_sidebar(&app, &["b", "a"], "a");
    calls.lock().unwrap().clear();

    // Switching uses that same snapshot, with no second workspace.get.
    app.show_workspace("b");
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(app.tab_view.n_pages(), 2);
    let first = app.tabs.borrow()["tab_b"].page.clone();
    assert_eq!(first.title(), "updated tab");
    assert!(first.is_loading());
    assert!(first.indicator_icon().is_some());
    let selected = app.tabs.borrow()["tab_b2"].page.clone();
    app.tab_view.set_selected_page(&selected);

    // Notifications stay per event even though the refresh is coalesced.
    emit(
        &app,
        "attention.created",
        json!({"pane_id": "pane_b", "attention": "unread"}),
    );
    emit(
        &app,
        "attention.updated",
        json!({"pane_id": "pane_b", "attention": "permission_required"}),
    );
    drain_refresh(&app);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| method == "pane.get")
            .count(),
        2
    );
    assert_eq!(calls.lock().unwrap().len(), 3);
    assert_eq!(app.tab_view.selected_page(), Some(selected.clone()));
    calls.lock().unwrap().clear();

    // A second dirty workspace joins the batch; its attention remains counted
    // when b clears. Active tab selection survives both batches.
    {
        let mut state = state.lock().unwrap();
        state.get_mut("a").unwrap()["panes"][0]["attention"] = json!("error");
        state.get_mut("b").unwrap()["panes"][0]["attention"] = json!("none");
        state.get_mut("b").unwrap()["panes"][0]["lifecycle"] = json!("idle");
    }
    emit(&app, "pane.updated", json!({"pane_id": "pane_a"}));
    emit(&app, "attention.cleared", json!({"pane_id": "pane_b"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_eq!(app.attention.count.text(), "1");
    assert!(first.indicator_icon().is_none());
    assert!(!first.is_loading());
    assert_eq!(app.tab_view.selected_page(), Some(selected.clone()));
    assert_sidebar(&app, &["a", "b"], "b");
    calls.lock().unwrap().clear();

    // Deletion payloads contain only ids; routing must use the old snapshot.
    {
        let mut state = state.lock().unwrap();
        let b = state.get_mut("b").unwrap();
        b["panes"] = json!([]);
        b["tabs"].as_array_mut().unwrap().remove(0);
        b["workspace"]["tabs"] = json!(["tab_b2"]);
    }
    emit(&app, "pane.closed", json!({"pane_id": "pane_b"}));
    emit(&app, "tab.closed", json!({"tab_id": "tab_b"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(app.tab_view.n_pages(), 1);
    assert_eq!(app.tab_view.selected_page(), Some(selected));
    assert_eq!(app.widgets.borrow().len(), 1);
    assert_eq!(app.widgets.borrow()["pane_a"].root, original_terminal);
    assert!(!app.widgets.borrow().contains_key("pane_b"));
    assert_eq!(app.attention.count.text(), "1");
    assert_sidebar(&app, &["a", "b"], "b");
    calls.lock().unwrap().clear();

    state.lock().unwrap().insert("c".into(), fixture("c"));
    emit(&app, "workspace.created", json!({"workspace": {"id": "c"}}));
    app.on_event(UiEvent::Disconnected);
    app.on_event(UiEvent::Reconnected);
    drain_refresh(&app);
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == "workspace.list")
            .count(),
        1
    );
    assert_eq!(calls.lock().unwrap().len(), 5);
    assert!(has_label(app.sidebar.widget.upcast_ref(), "c"));
    assert!(!app.banner.is_revealed());
    assert_sidebar(&app, &["a", "c", "b"], "b");
    calls.lock().unwrap().clear();

    state.lock().unwrap().remove("b");
    emit(&app, "workspace.closed", json!({"workspace_id": "b"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 4);
    assert_eq!(app.active_ws_id().as_deref(), Some("a"));
    assert_eq!(app.title.text(), "a");
    assert!(!has_label(app.sidebar.widget.upcast_ref(), "b"));
    assert_sidebar(&app, &["a", "c"], "a");
    calls.lock().unwrap().clear();

    // The selected workspace sinks when another needs attention;
    // selection stays on it even though its own row moved.
    {
        let mut state = state.lock().unwrap();
        state.get_mut("a").unwrap()["panes"][0]["attention"] = json!("none");
        state.get_mut("c").unwrap()["panes"][0]["attention"] = json!("error");
    }
    emit(&app, "pane.updated", json!({"pane_id": "pane_a"}));
    emit(&app, "pane.updated", json!({"pane_id": "pane_c"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 2);
    assert_sidebar(&app, &["c", "a"], "a");
    calls.lock().unwrap().clear();

    // Only the other workspace changes, but the selected row still
    // moves (back to the top); the active workspace is untouched so
    // no show_workspace re-selects it — update() must preserve it.
    {
        let mut state = state.lock().unwrap();
        state.get_mut("c").unwrap()["panes"][0]["attention"] = json!("none");
    }
    emit(&app, "pane.updated", json!({"pane_id": "pane_c"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_sidebar(&app, &["a", "c"], "a");
    {
        let mut state = state.lock().unwrap();
        let a = state.get_mut("a").unwrap();
        a["panes"] = json!([]);
        a["tabs"] = json!([]);
        a["workspace"]["tabs"] = json!([]);
    }
    emit(&app, "pane.closed", json!({"pane_id": "pane_a"}));
    drain_refresh(&app);
    assert!(!app.widgets.borrow().contains_key("pane_a"));

    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    drop(app);
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn stalled_action_keeps_the_gtk_main_loop_responsive() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call { method, reply, .. } = request {
                let result = match method.as_str() {
                    "workspace.list" => Ok(json!({"workspaces": [fixture("a")["workspace"]]})),
                    "workspace.get" => Ok(fixture("a")),
                    "tab.create" => {
                        std::thread::sleep(Duration::from_millis(250));
                        Err("simulated stalled reply".into())
                    }
                    "test.stop" => {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    _ => Ok(json!({})),
                };
                let _ = reply.send(result);
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    drain_refresh(&app);
    let started = Instant::now();
    app.window.lookup_action("new-tab").unwrap().activate(None);
    let activation_time = started.elapsed();
    // An idle probe runs while tab.create is still awaiting its delayed reply.
    let responsive = Rc::new(Cell::new(false));
    let probe = responsive.clone();
    glib::idle_add_local_once(move || probe.set(true));
    while !responsive.get() {
        glib::MainContext::default().iteration(true);
    }
    assert!(started.elapsed() < Duration::from_millis(100));
    drain_refresh(&app);
    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    worker.join().unwrap();
    assert!(
        activation_time < Duration::from_millis(100),
        "GTK action blocked for {activation_time:?}"
    );
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn events_received_during_refresh_are_applied_in_the_next_batch() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let (waiting, held) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut reads = 0;
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call { method, reply, .. } = request {
                let result = match method.as_str() {
                    "workspace.list" => json!({"workspaces": [fixture("a")["workspace"]]}),
                    "workspace.get" => {
                        reads += 1;
                        let mut snapshot = fixture("a");
                        snapshot["panes"][0]["last_message"] = json!(if reads == 1 {
                            "old message"
                        } else {
                            "new message"
                        });
                        if reads == 1 {
                            waiting.send(()).unwrap();
                            resume.recv_timeout(Duration::from_secs(2)).unwrap();
                        }
                        snapshot
                    }
                    "test.stop" => {
                        let _ = reply.send(Ok(json!({"reads": reads})));
                        break;
                    }
                    _ => json!({}),
                };
                let _ = reply.send(Ok(result));
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    let deadline = Instant::now() + Duration::from_secs(1);
    while held.try_recv().is_err() {
        assert!(Instant::now() < deadline);
        glib::MainContext::default().iteration(false);
        std::thread::yield_now();
    }
    // The cache can be borrowed while its remote read awaits, and this
    // invalidation must survive application of the first stale snapshot.
    emit(
        &app,
        "pane.updated",
        json!({"workspace_id": "a", "pane_id": "pane_a"}),
    );
    release.send(()).unwrap();
    drain_refresh(&app);
    assert!(has_label(app.sidebar.widget.upcast_ref(), "new message"));
    let result = glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    assert_eq!(result["reads"], 2);
    app.window.destroy();
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn delayed_new_tab_keeps_the_workspace_selected_after_the_action() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let (waiting, held) = std::sync::mpsc::channel();
    let (release, resume) = std::sync::mpsc::channel();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call {
                method,
                params,
                reply,
                ..
            } = request
            {
                recorded
                    .lock()
                    .unwrap()
                    .push((method.clone(), params.clone()));
                let result = match method.as_str() {
                    "workspace.list" => Ok(json!({"workspaces": [
                        fixture("a")["workspace"], fixture("b")["workspace"]
                    ]})),
                    "workspace.get" => Ok(fixture(params["workspace_id"].as_str().unwrap())),
                    "tab.create" => {
                        waiting.send(()).unwrap();
                        resume.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(json!({"tab": {"id": "tab_new_a"}}))
                    }
                    "pane.spawn" => Err("simulated spawn failure".into()),
                    "test.stop" => {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    _ => Ok(json!({})),
                };
                let _ = reply.send(result);
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    drain_refresh(&app);
    app.show_workspace("a");
    app.window.lookup_action("new-tab").unwrap().activate(None);
    let deadline = Instant::now() + Duration::from_secs(1);
    while held.try_recv().is_err() {
        assert!(Instant::now() < deadline);
        glib::MainContext::default().iteration(false);
        std::thread::yield_now();
    }
    app.show_workspace("b");
    assert_eq!(app.current_pane_id().as_deref(), Some("pane_b"));
    release.send(()).unwrap();
    drain_refresh(&app);
    let active = app.active_ws_id();
    let selected_tab = app.gui_tab.borrow().clone();
    let current_pane = app.current_pane_id();
    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    worker.join().unwrap();

    let calls = calls.lock().unwrap();
    let created = calls
        .iter()
        .find(|(method, _)| method == "tab.create")
        .unwrap();
    assert_eq!(created.1["workspace_id"], "a");
    let spawned = calls
        .iter()
        .find(|(method, _)| method == "pane.spawn")
        .unwrap();
    assert_eq!(spawned.1["workspace_id"], "a");
    assert_eq!(spawned.1["tab_id"], "tab_new_a");
    assert_eq!(active.as_deref(), Some("b"));
    assert_eq!(selected_tab.as_deref(), Some("tab_b"));
    assert_eq!(current_pane.as_deref(), Some("pane_b"));
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn integration_setup_notice_is_visible_in_light_and_dark() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let style = adw::StyleManager::default();
    let original = style.color_scheme();
    for scheme in [adw::ColorScheme::ForceLight, adw::ColorScheme::ForceDark] {
        style.set_color_scheme(scheme);
        let (actor, _requests) = IpcHandle::test_channel();
        let (ui, _) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(&application, actor, ui);
        app.window.present();
        let notice = "Codex: review and trust the Signaltty hooks in /hooks <literal>";
        app.show_integration_notice(&json!({"integration":{"notice":notice}}));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !has_label(app.window.upcast_ref(), notice) {
            assert!(Instant::now() < deadline, "integration notice not visible");
            glib::MainContext::default().iteration(true);
        }
        app.window.destroy();
    }
    style.set_color_scheme(original);
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn pane_zoom_keeps_hidden_terminals_and_restores_latest_ratios() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().unwrap())
        .add_resource_path("/dev/signaltty/gui/icons");
    let app = App::new(&application, actor, ui);
    let mut snapshot: crate::refresh::Snapshot = serde_json::from_value(fixture("a")).unwrap();
    let mut sibling = snapshot.panes[0].clone();
    sibling.id = "sibling".into();
    snapshot.panes.push(sibling);
    snapshot.tabs[0].layout = Some(Layout::Split {
        dir: SplitDir::Right,
        ratio: 0.3,
        first: Box::new(Layout::Pane {
            pane_id: "pane_a".into(),
        }),
        second: Box::new(Layout::Pane {
            pane_id: "sibling".into(),
        }),
    });
    {
        let mut model = app.model.borrow_mut();
        model.cache.workspaces = vec![snapshot.workspace.clone()];
        model.cache.snapshots.insert("a".into(), snapshot.clone());
    }
    app.show_workspace("a");
    app.on_pane_focused("pane_a");
    let original = app.widgets.borrow()["pane_a"].root.clone();
    let hidden = app.widgets.borrow()["sibling"].root.clone();
    let original_vte = find_widget::<vte4::Terminal>(original.upcast_ref()).unwrap();
    let hidden_vte = find_widget::<vte4::Terminal>(hidden.upcast_ref()).unwrap();
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.zoom-pane", None).unwrap();
    assert_eq!(app.widgets.borrow()["pane_a"].root, original);
    assert_eq!(app.widgets.borrow()["sibling"].root, hidden);
    assert!(app.paned_widgets.borrow().is_empty());
    app.widgets.borrow()["sibling"].feed(b"hidden output remains alive\r\n");
    if let Some(Layout::Split { ratio, .. }) = &mut app.model.borrow_mut().tabs[0].layout {
        *ratio = 0.7;
    }
    app.render_tabs();
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.zoom-pane", None).unwrap();
    assert_eq!(app.widgets.borrow()["sibling"].root, hidden);
    assert_eq!(app.widgets.borrow()["pane_a"].root, original);
    assert!(!app.paned_widgets.borrow().is_empty());
    let tab = app.tabs.borrow()["tab_a"].layout.clone().unwrap();
    assert!(matches!(tab, Layout::Split { ratio, .. } if ratio == 0.7));
    app.window.set_default_size(900, 620);
    app.present();
    app.widgets.borrow()["pane_a"]
        .feed(b"Checking local agent workflows\r\nFound [literal].* marker\r\n");
    wait_ui(|| vte4::prelude::TerminalExt::cursor_position(&original_vte).1 >= 2);
    wait_ui(|| vte4::prelude::TerminalExt::cursor_position(&hidden_vte).1 >= 1);
    let style = adw::StyleManager::default();
    let original_scheme = style.color_scheme();
    let settings = gtk4::Settings::default().unwrap();
    let original_motion = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        style.set_color_scheme(scheme);
        app.widgets.borrow()["pane_a"].focus();
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.search-terminal", None)
            .unwrap();
        let pane = app.widgets.borrow()["pane_a"].clone();
        let entry = find_widget::<gtk4::SearchEntry>(pane.root.upcast_ref()).unwrap();
        entry.set_text("[literal].*");
        button_with_tooltip(pane.root.upcast_ref(), "Next match (Enter)")
            .unwrap()
            .emit_clicked();
        assert!(vte4::prelude::TerminalExt::has_selection(&original_vte));
        let minimum = pane.root.measure(gtk4::Orientation::Horizontal, -1).0;
        assert!(minimum <= 320, "terminal search needs {minimum}px");
        capture_workflow(&app.window, &format!("search-{theme}"));
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.zoom-pane", None).unwrap();
        capture_workflow(&app.window, &format!("zoom-{theme}"));
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.zoom-pane", None).unwrap();
        assert_eq!(app.widgets.borrow()["pane_a"].root, original);
        assert_eq!(app.widgets.borrow()["sibling"].root, hidden);
        assert_eq!(
            find_widget::<vte4::Terminal>(original.upcast_ref()).unwrap(),
            original_vte
        );
        assert_eq!(
            find_widget::<vte4::Terminal>(hidden.upcast_ref()).unwrap(),
            hidden_vte
        );
        pane.close_search();
    }
    style.set_color_scheme(original_scheme);
    settings.set_gtk_enable_animations(original_motion);
    app.window.destroy();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn theme_and_appearance_swapping_updates_window_classes() {
    // The setters persist gui.json; keep the user's own file out of it.
    let config = std::env::temp_dir().join(format!("signaltty-theme-test-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &config);
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.window.present();

    // Default startup has theme-signal
    assert!(app.window.has_css_class("theme-signal"));

    // Switch to Ocean + Dark
    app.set_theme(signaltty_core::theme::Theme::Ocean);
    app.set_appearance(signaltty_core::theme::Appearance::Dark);
    while glib::MainContext::default().iteration(false) {}

    assert!(app.window.has_css_class("theme-ocean"));
    assert!(!app.window.has_css_class("theme-signal"));
    assert!(app.window.has_css_class("dark"));

    // Switch back to Signal + Light
    app.set_theme(signaltty_core::theme::Theme::Signal);
    app.set_appearance(signaltty_core::theme::Appearance::Light);
    while glib::MainContext::default().iteration(false) {}

    assert!(app.window.has_css_class("theme-signal"));
    assert!(!app.window.has_css_class("theme-ocean"));
    assert!(!app.window.has_css_class("dark"));

    // Sidebar width preference applies, clamps, and persists
    app.set_sidebar_width(380);
    assert_eq!(app.preference().sidebar_width, Some(380));
    assert_eq!(app.split_view.min_sidebar_width(), 380.0);
    assert_eq!(app.split_view.max_sidebar_width(), 380.0);

    // Sidebar width clamp below min
    app.set_sidebar_width(100);
    assert_eq!(app.preference().sidebar_width, Some(200));
    assert_eq!(app.split_view.min_sidebar_width(), 200.0);
    assert_eq!(app.split_view.max_sidebar_width(), 200.0);

    // Sidebar width clamp above max
    app.set_sidebar_width(900);
    assert_eq!(app.preference().sidebar_width, Some(560));
    assert_eq!(app.split_view.min_sidebar_width(), 560.0);
    assert_eq!(app.split_view.max_sidebar_width(), 560.0);

    // Persisted file has the updated width
    let loaded = crate::preferences::load_preference();
    assert_eq!(loaded.sidebar_width, Some(560));

    assert_eq!(app.split_view.sidebar_width_unit(), adw::LengthUnit::Px);
    let handle = try_descendant(app.sidebar_overlay.upcast_ref(), "sidebar-handle").unwrap();
    assert!(handle.is_focusable());
    app.split_view.set_show_sidebar(true);
    wait_ui(|| handle.is_mapped());
    assert!(handle.grab_focus());
    let keys = handle
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .expect("sidebar handle has keyboard controls");
    let press = |key: gtk4::gdk::Key| {
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&key, &0u32, &gtk4::gdk::ModifierType::empty()],
        )
    };
    for (direction, grow, shrink) in [
        (
            gtk4::TextDirection::Ltr,
            gtk4::gdk::Key::Right,
            gtk4::gdk::Key::Left,
        ),
        (
            gtk4::TextDirection::Rtl,
            gtk4::gdk::Key::Left,
            gtk4::gdk::Key::Right,
        ),
    ] {
        app.sidebar_overlay.set_direction(direction);
        app.set_sidebar_width(380);
        assert!(press(grow));
        assert!(press(grow)); // Repeats before the next layout must accumulate.
        assert_eq!(app.split_view.min_sidebar_width(), 400.0);
        assert!(press(shrink));
        assert_eq!(app.split_view.max_sidebar_width(), 390.0);
        assert_eq!(
            crate::preferences::load_preference().sidebar_width,
            Some(390)
        );
        app.set_sidebar_width(560);
        assert!(press(grow));
        assert_eq!(app.preference().sidebar_width, Some(560));
        app.set_sidebar_width(200);
        assert!(press(shrink));
        assert_eq!(app.preference().sidebar_width, Some(200));
        assert!(!press(gtk4::gdk::Key::Tab));
    }

    app.window.destroy();
    std::env::remove_var("XDG_CONFIG_HOME");
    let _ = std::fs::remove_dir_all(&config);
}

fn find_widget<T: glib::object::IsA<gtk4::Widget> + glib::types::StaticType>(
    root: &gtk4::Widget,
) -> Option<T> {
    if let Ok(widget) = root.clone().downcast::<T>() {
        return Some(widget);
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget::<T>(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn button_with_label(root: &gtk4::Widget, label: &str) -> Option<gtk4::Button> {
    if let Some(button) = root.downcast_ref::<gtk4::Button>() {
        if button.label().as_deref() == Some(label) {
            return Some(button.clone());
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(button) = button_with_label(&widget, label) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

fn action_row_with_title(root: &gtk4::Widget, title: &str) -> Option<adw::ActionRow> {
    if let Some(row) = root.downcast_ref::<adw::ActionRow>() {
        if row.title() == title {
            return Some(row.clone());
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(row) = action_row_with_title(&widget, title) {
            return Some(row);
        }
        child = widget.next_sibling();
    }
    None
}

fn collect_entries(root: &gtk4::Widget, entries: &mut Vec<adw::EntryRow>) {
    if let Some(entry) = root.downcast_ref::<adw::EntryRow>() {
        entries.push(entry.clone());
        return;
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        collect_entries(&widget, entries);
        child = widget.next_sibling();
    }
}

fn wait_ui(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(Instant::now() < deadline, "UI operation did not complete");
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn capture_workflow(window: &adw::ApplicationWindow, name: &str) {
    let Some(directory) = std::env::var_os("SIGNALTTY_UI_EVIDENCE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let deadline = Instant::now() + Duration::from_millis(120);
    while Instant::now() < deadline {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let node = loop {
        let paintable = gtk4::WidgetPaintable::new(Some(window));
        let snapshot = gtk4::Snapshot::new();
        paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
        if let Some(node) = snapshot.to_node() {
            break node;
        }
        assert!(Instant::now() < deadline, "window did not paint");
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    };
    let renderer = gtk4::gsk::CairoRenderer::new();
    renderer
        .realize_for_display(&gtk4::prelude::WidgetExt::display(window))
        .unwrap();
    let texture = renderer.render_texture(&node, None);
    renderer.unrealize();
    texture
        .save_to_png(directory.join(format!("{name}.png")))
        .unwrap();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn workspace_header_context_survives_shell_agent_and_empty_transitions() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().unwrap())
        .add_resource_path("/dev/signaltty/gui/icons");
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let (actor, mut requests) = IpcHandle::test_channel();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call { method, reply, .. } = request {
                let value = match method.as_str() {
                    "workspace.list" => json!({"workspaces": []}),
                    "task.list" => json!({"tasks": []}),
                    "test.stop" => {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    _ => Value::Null,
                };
                let _ = reply.send(Ok(value));
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let style = adw::StyleManager::default();
    let original_scheme = style.color_scheme();
    let app = App::new(&application, actor.clone(), ui);
    app.apply_preference(signaltty_core::theme::GuiPreference::default());
    app.window.set_default_size(720, 600);
    app.window.present();
    let separator = try_descendant(app.window.upcast_ref(), "crumb-sep").unwrap();
    assert!(!separator.is_visible(), "startup has no context separator");
    let mut shell = fixture("shell");
    shell["workspace"]["name"] = json!("Signaltty");
    shell["workspace"]["cwd"] = json!("/tmp/signaltty <literal>&");
    shell["workspace"]["git"] = json!({"branch": "feature/header"});
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    wait_ui(|| app.title.is_mapped());
    style.set_color_scheme(adw::ColorScheme::ForceLight);
    capture_workflow(&app.window, "header-shell-light");
    style.set_color_scheme(adw::ColorScheme::ForceDark);
    capture_workflow(&app.window, "header-shell-dark");
    assert_eq!(
        app.title_context.text(),
        "feature/header · /tmp/signaltty <literal>&"
    );
    assert!(app.title_context.is_visible() && separator.is_visible());
    assert_eq!(
        app.title_context.tooltip_text().as_deref(),
        Some("feature/header · /tmp/signaltty <literal>&")
    );
    shell["workspace"]["git"]["branch"] = json!("   ");
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    assert_eq!(app.title_context.text(), "/tmp/signaltty <literal>&");
    shell["workspace"]["git"]["branch"] = Value::Null;
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    assert_eq!(app.title_context.text(), "/tmp/signaltty <literal>&");
    shell["panes"][0]["agent"] = json!({"kind": "claude"});
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    assert_eq!(app.title_context.text(), "Claude");
    assert_eq!(
        app.title_context.tooltip_text().as_deref(),
        Some("/tmp/signaltty <literal>&")
    );
    shell["workspace"]["git"]["branch"] = json!("feature/header");
    let home_path = format!("{}/signaltty", std::env::var("HOME").unwrap());
    shell["workspace"]["cwd"] = json!(home_path);
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    assert_eq!(app.title_context.text(), "Claude");
    assert_eq!(
        app.title_context.tooltip_text().as_deref(),
        Some("feature/header · ~/signaltty")
    );
    capture_workflow(&app.window, "header-agent-dark");
    shell["panes"][0]["agent"] = json!({"kind": "none"});
    shell["workspace"]["cwd"] = json!("");
    shell["workspace"]["git"]["branch"] = Value::Null;
    app.model.borrow_mut().cache.snapshots.insert(
        "shell".into(),
        serde_json::from_value(shell.clone()).unwrap(),
    );
    app.show_workspace("shell");
    assert!(!app.title_context.is_visible() && !separator.is_visible());

    let settings = gtk4::Settings::default().unwrap();
    let original_font = settings.gtk_font_name();
    shell["workspace"]["name"] =
        json!("A workspace with a very long literal <name>& that must shrink");
    shell["workspace"]["cwd"] =
        json!("/tmp/an/extremely/long/path/that/must/ellipsize/inside/the/header");
    shell["workspace"]["git"]["branch"] = json!("feature/a-very-long-branch-name");
    app.model
        .borrow_mut()
        .cache
        .snapshots
        .insert("shell".into(), serde_json::from_value(shell).unwrap());
    app.show_workspace("shell");
    app.window.set_default_size(360, 600);
    app.split_view.set_show_sidebar(false);
    for (scheme, name, font) in [
        (
            adw::ColorScheme::ForceLight,
            "header-narrow-light",
            "Sans 11",
        ),
        (
            adw::ColorScheme::ForceDark,
            "header-narrow-dark-large",
            "Sans 18",
        ),
    ] {
        style.set_color_scheme(scheme);
        settings.set_gtk_font_name(Some(font));
        wait_ui(|| app.window.width() > 0 && app.window.width() <= 360);
        let width = f64::from(app.window.width());
        capture_workflow(&app.window, name);
        for widget in [
            app.title.clone().upcast::<gtk4::Widget>(),
            app.title_context.clone().upcast(),
            button_with_tooltip(app.window.upcast_ref(), "New Tab (Ctrl+Shift+T)")
                .unwrap()
                .upcast(),
            try_descendant(app.window.upcast_ref(), "crumb-sep").unwrap(),
        ] {
            let bounds = widget_bounds(&widget, app.window.upcast_ref());
            assert!(
                bounds.w > 0.0 && bounds.x >= 0.0 && bounds.x + bounds.w <= width,
                "header child outside narrow window"
            );
        }
        let context = widget_bounds(app.title_context.upcast_ref(), app.window.upcast_ref());
        let new_tab =
            button_with_tooltip(app.window.upcast_ref(), "New Tab (Ctrl+Shift+T)").unwrap();
        let new_tab_bounds = widget_bounds(new_tab.upcast_ref(), app.window.upcast_ref());
        assert!(
            context.x + context.w <= new_tab_bounds.x,
            "context overlaps New Tab"
        );
        assert!(app.title.layout().is_ellipsized() || app.title_context.layout().is_ellipsized());
        assert_eq!(app.title_context.tooltip_text().as_deref(), Some("feature/a-very-long-branch-name · /tmp/an/extremely/long/path/that/must/ellipsize/inside/the/header"));
        let sidebar_toggle =
            find_matching_widget::<gtk4::ToggleButton>(app.window.upcast_ref(), &|button| {
                button.tooltip_text().as_deref() == Some("Toggle Sidebar (F9)")
            })
            .unwrap();
        let sidebar_bounds = widget_bounds(sidebar_toggle.upcast_ref(), app.window.upcast_ref());
        assert!(
            sidebar_toggle.is_mapped()
                && sidebar_bounds.x >= 0.0
                && sidebar_bounds.x + sidebar_bounds.w <= width
        );
        let menu = find_widget::<gtk4::MenuButton>(app.window.upcast_ref()).unwrap();
        let bounds = widget_bounds(menu.upcast_ref(), app.window.upcast_ref());
        assert!(menu.is_mapped() && bounds.x + bounds.w <= width);
    }
    settings.set_gtk_font_name(original_font.as_deref());
    style.set_color_scheme(original_scheme);
    glib::MainContext::default().block_on(app.refresh_async());
    assert_eq!(app.title.text(), "signaltty");
    assert!(!app.title_mark.is_visible());
    assert!(!app.title_context.is_visible() && !separator.is_visible());
    assert!(app.title_context.text().is_empty() && app.title_context.tooltip_text().is_none());
    glib::MainContext::default()
        .block_on(actor.call("test.stop", json!({})))
        .unwrap();
    worker.join().unwrap();
    app.window.destroy();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn palette_keyboard_selection_stays_visible_while_search_keeps_focus() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(720, 600);
    window.present();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let workspaces = (0..40)
        .map(|i| {
            let mut value = fixture(&format!("workspace-{i:02}"))["workspace"].clone();
            value["name"] = json!(format!("Workspace {i:02}"));
            serde_json::from_value(value).unwrap()
        })
        .collect::<Vec<_>>();
    let activated = Rc::new(RefCell::new(None));
    let chosen = activated.clone();
    let dialog = crate::palette::present(
        &window,
        &workspaces,
        move |id| *chosen.borrow_mut() = Some(id.to_string()),
        || {},
    );
    let body = dialog.child().unwrap();
    let entry = find_widget::<gtk4::SearchEntry>(&body).unwrap();
    let list = find_widget::<gtk4::ListBox>(&body).unwrap();
    let scroll = find_widget::<gtk4::ScrolledWindow>(&body).unwrap();
    wait_ui(|| scroll.height() > 0 && list.height() > scroll.height());
    let search_focus = gtk4::prelude::GtkWindowExt::focus(&window).unwrap();
    assert!(search_focus.is_ancestor(&entry));
    let keys = entry
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
        .unwrap();
    let press = |key: gtk4::gdk::Key| {
        keys.emit_by_name::<bool>(
            "key-pressed",
            &[&key, &0u32, &gtk4::gdk::ModifierType::empty()],
        )
    };
    for _ in 0..30 {
        assert!(press(gtk4::gdk::Key::Down));
        while glib::MainContext::default().iteration(false) {}
    }
    assert_eq!(list.selected_row().unwrap().index(), 30);
    wait_ui(|| {
        list.selected_row()
            .unwrap()
            .compute_bounds(&scroll)
            .is_some_and(|bounds| {
                bounds.y() >= -1.0 && bounds.y() + bounds.height() <= scroll.height() as f32 + 1.0
            })
    });
    capture_workflow(&window, "palette-keyboard-selection");
    let selected = list.selected_row().unwrap();
    let bounds = selected.compute_bounds(&scroll).unwrap();
    assert!(
        bounds.y() >= -1.0 && bounds.y() + bounds.height() <= scroll.height() as f32 + 1.0,
        "selected result must be visible: y={}, height={}, viewport={}",
        bounds.y(),
        bounds.height(),
        scroll.height()
    );
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window),
        Some(search_focus)
    );
    for _ in 0..100 {
        press(gtk4::gdk::Key::Down);
    }
    let last = list.selected_row().unwrap().index();
    assert!(list.row_at_index(last + 1).is_none());
    press(gtk4::gdk::Key::Down);
    assert_eq!(list.selected_row().unwrap().index(), last);
    for _ in 0..100 {
        press(gtk4::gdk::Key::Up);
    }
    assert_eq!(list.selected_row().unwrap().index(), 0);
    press(gtk4::gdk::Key::Up);
    assert_eq!(list.selected_row().unwrap().index(), 0);
    for _ in 0..30 {
        press(gtk4::gdk::Key::Down);
    }
    entry.set_text("Workspace 3");
    wait_ui(|| list.height() < 1000);
    let bounds = list
        .selected_row()
        .unwrap()
        .compute_bounds(&scroll)
        .unwrap();
    assert!(
        bounds.y() >= -1.0,
        "first filtered result must stay visible: y={}",
        bounds.y()
    );
    entry.set_text("Workspace 00");
    wait_ui(|| list.selected_row().is_some_and(|row| row.index() == 0));
    wait_ui(|| {
        list.selected_row()
            .unwrap()
            .compute_bounds(&scroll)
            .is_some_and(|bounds| bounds.y() >= -1.0)
    });
    entry.set_text("Workspace 3");
    entry.emit_activate();
    wait_ui(|| activated.borrow().as_deref() == Some("workspace-30"));
    window.close();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn palette_empty_results_and_shortcuts_fit_narrow_appearances() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(360, 640);
    window.present();
    let settings = gtk4::Settings::default().unwrap();
    let old_font = settings.gtk_font_name();
    let old_motion = settings.is_gtk_enable_animations();
    let old_scheme = adw::StyleManager::default().color_scheme();
    settings.set_gtk_enable_animations(false);
    let activated = Rc::new(RefCell::new(None));
    let mut value = fixture("long")["workspace"].clone();
    value["name"] = json!("Workspace <literal> & a very long name for parallel coding agents");
    value["cwd"] = json!(format!("/tmp/{}", "long-unbroken-directory".repeat(8)));
    let workspaces = [serde_json::from_value(value).unwrap()];
    for (name, scheme, large) in [
        ("light", adw::ColorScheme::ForceLight, false),
        ("dark", adw::ColorScheme::ForceDark, false),
        ("large-high-contrast", adw::ColorScheme::ForceLight, true),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        if scheme == adw::ColorScheme::ForceDark {
            window.add_css_class("dark");
        } else {
            window.remove_css_class("dark");
        }
        if large {
            settings.set_gtk_font_name(Some("Sans 18"));
            window.add_css_class("high-contrast");
        }
        let chosen = activated.clone();
        let dialog = crate::palette::present(
            &window,
            &workspaces,
            move |id| *chosen.borrow_mut() = Some(id.to_string()),
            || {},
        );
        let body = dialog.child().unwrap();
        let entry = find_widget::<gtk4::SearchEntry>(&body).unwrap();
        let list = find_widget::<gtk4::ListBox>(&body).unwrap();
        wait_ui(|| dialog.width() > 0);
        entry.set_text("no-such-command-or-workspace");
        capture_workflow(&window, &format!("palette-empty-narrow-{name}"));
        assert!(
            has_label(&body, "No Matches"),
            "empty search needs explicit feedback"
        );
        let empty = find_widget::<adw::StatusPage>(&body).unwrap();
        let scroll = find_widget::<gtk4::ScrolledWindow>(&body).unwrap();
        assert!(empty.is_mapped() && !scroll.is_visible());
        assert!(has_label(&body, "Try another command or workspace name."));
        assert!(list.selected_row().is_none());
        entry.emit_activate();
        assert!(window.visible_dialog().is_some());
        assert!(activated.borrow().is_none());
        entry.set_text("New Workspace");
        wait_ui(|| list.selected_row().is_some());
        wait_ui(|| scroll.is_mapped() && !empty.is_visible());
        let row = list.selected_row().unwrap();
        let hint = find_matching_widget::<gtk4::Label>(row.upcast_ref(), &|label| {
            label.text().contains("Ctrl") && label.text().contains('N')
        })
        .expect("command result should teach its existing shortcut");
        capture_workflow(&window, &format!("palette-shortcut-narrow-{name}"));
        let title = find_matching_widget::<gtk4::Label>(row.upcast_ref(), &|label| {
            label.text() == "New Workspace"
        })
        .unwrap();
        assert!(
            !title.layout().is_ellipsized(),
            "shortcut must not truncate a short command name"
        );
        assert!(window.width() <= 360);
        assert!(dialog.width() <= window.width());
        let bounds = hint.compute_bounds(&window).unwrap();
        assert!(bounds.x() >= 0.0 && bounds.x() + bounds.width() <= window.width() as f32);
        entry.set_text("");
        wait_ui(|| list.selected_row().is_some_and(|row| row.index() == 0));
        capture_workflow(&window, &format!("palette-results-narrow-{name}"));
        assert!(window.width() <= 360);
        assert!(has_label(&body, workspaces[0].name.as_str()));
        entry.emit_activate();
        wait_ui(|| activated.borrow().as_deref() == Some("long"));
        activated.replace(None);
        wait_ui(|| window.visible_dialog().is_none());
    }
    settings.set_gtk_font_name(old_font.as_deref());
    settings.set_gtk_enable_animations(old_motion);
    adw::StyleManager::default().set_color_scheme(old_scheme);
    window.close();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn navigation_palette_fast_enter_and_git_dialogs_use_native_controls() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let state = Arc::new(Mutex::new(BTreeMap::from([
        ("a".to_string(), fixture("a")),
        ("b".to_string(), fixture("b")),
    ])));
    let server = state.clone();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let recorded = calls.clone();
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call {
                method,
                params,
                reply,
                ..
            } = request
            {
                if method == "test.stop" {
                    let _ = reply.send(Ok(Value::Null));
                    break;
                }
                recorded
                    .lock()
                    .unwrap()
                    .push((method.clone(), params.clone()));
                let mut state = server.lock().unwrap();
                let value = match method.as_str() {
                    "workspace.list" => {
                        json!({"workspaces": state.values().map(|s| s["workspace"].clone()).collect::<Vec<_>>()})
                    }
                    "workspace.get" => state[params["workspace_id"].as_str().unwrap()].clone(),
                    "workspace.rename" => {
                        let ws = &mut state
                            .get_mut(params["workspace_id"].as_str().unwrap())
                            .unwrap()["workspace"];
                        ws["name"] = params["name"].clone();
                        json!({"workspace": ws})
                    }
                    "workspace.diff" => json!({"added": 12, "removed": 3, "files": [
                        {"path": "src/<changed>&.rs", "added": 12, "removed": 3, "untracked": false, "binary": false},
                        {"path": "assets/banner.png", "added": 0, "removed": 0, "untracked": false, "binary": true},
                        {"path": "notes/new.txt", "added": 0, "removed": 0, "untracked": true, "binary": false}]}),
                    "worktree.list" => json!({"worktrees": [
                        {"path": "/tmp/main <checkout>&", "branch": "main", "main": true, "locked": false, "workspace_id": "a"},
                        {"path": "/tmp/parallel agent/feature <checkout>&", "branch": "feature/local-workflows", "main": false, "locked": false, "workspace_id": null}]}),
                    "worktree.open" => {
                        json!({"workspace": state["b"]["workspace"], "reused": true})
                    }
                    "worktree.create" => {
                        let mut created = fixture("c");
                        created["workspace"]["cwd"] = params["path"].clone();
                        created["panes"] = json!([]);
                        created["tabs"] = json!([]);
                        created["workspace"]["tabs"] = json!([]);
                        created["workspace"]["active_tab_id"] = Value::Null;
                        state.insert("c".into(), created.clone());
                        json!({"workspace": created["workspace"], "reused": false})
                    }
                    "pane.spawn" => {
                        let mut created = fixture("c");
                        created["workspace"]["cwd"] = json!("/tmp/parallel <new>&");
                        state.insert("c".into(), created.clone());
                        json!({"pane": created["panes"][0]})
                    }
                    "pane.get" => {
                        let pane = state
                            .values()
                            .flat_map(|value| value["panes"].as_array().unwrap())
                            .find(|pane| pane["id"] == params["pane_id"])
                            .cloned()
                            .unwrap();
                        json!({"pane": pane})
                    }
                    "worktree.remove" => {
                        let _ =
                            reply.send(Err("WORKTREE_DIRTY: checkout has local changes".into()));
                        continue;
                    }
                    "pane.mark_seen" | "pane.resize" => json!({}),
                    "task.list" => json!({"tasks": []}),
                    _ => panic!("unexpected request {method}"),
                };
                let _ = reply.send(Ok(value));
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().unwrap())
        .add_resource_path("/dev/signaltty/gui/icons");
    let app = App::new(&application, actor, ui);
    app.window.set_default_size(920, 680);
    app.present();
    app.refresh();
    drain_refresh(&app);
    let original = app.widgets.borrow()["pane_a"].root.clone();
    let style = adw::StyleManager::default();
    let original_scheme = style.color_scheme();
    let settings = gtk4::Settings::default().unwrap();
    let original_motion = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        style.set_color_scheme(scheme);
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.command-palette", None)
            .unwrap();
        let dialog = app.window.visible_dialog().unwrap();
        let entry = find_widget::<gtk4::SearchEntry>(&dialog.child().unwrap()).unwrap();
        let weak_entry = entry.downgrade();
        let weak_list = find_widget::<gtk4::ListBox>(&dialog.child().unwrap())
            .unwrap()
            .downgrade();
        capture_workflow(&app.window, &format!("palette-{theme}"));
        entry.set_text("b");
        entry.emit_activate();
        wait_ui(|| {
            app.window.visible_dialog().is_none() && app.active_ws_id().as_deref() == Some("b")
        });
        drop(entry);
        drop(dialog);
        wait_ui(|| weak_entry.upgrade().is_none() && weak_list.upgrade().is_none());
        // Changes docks beside the terminals instead of covering them.
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-changes", None).unwrap();
        assert!(app.changes_split.shows_sidebar());
        assert!(app.window.visible_dialog().is_none());
        let panel = app.changes.widget.clone().upcast::<gtk4::Widget>();
        wait_ui(|| has_label(&panel, "Changes against HEAD · 3 files · +12 −3"));
        assert!(has_label(&panel, "src"), "directory opens its group");
        assert!(action_row_with_title(&panel, "<changed>&.rs").is_some());
        assert!(has_label(&panel, "+12") && has_label(&panel, "−3"));
        assert!(has_label(&panel, "Binary"));
        assert!(has_label(&panel, "Untracked"));
        // Agent events re-show the active workspace; that must not re-run git.
        let reads = || {
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "workspace.diff")
                .count()
        };
        let before = reads();
        let active = app.active_ws_id().unwrap();
        app.show_workspace_internal(&active, false);
        let settle = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while std::time::Instant::now() < settle {
            glib::MainContext::default().iteration(false);
        }
        assert_eq!(reads(), before, "re-showing a workspace re-read its diff");
        capture_workflow(&app.window, &format!("changes-{theme}"));
        app.changes.focus();
        assert!(
            gtk4::prelude::GtkWindowExt::focus(&app.window).is_some_and(|f| f.is_ancestor(&panel))
        );
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-changes", None).unwrap();
        assert!(!app.changes_split.shows_sidebar());
        let focus = gtk4::prelude::GtkWindowExt::focus(&app.window);
        assert!(
            focus.is_some_and(|focus| focus.type_().name() == "VteTerminal"),
            "closing the panel returns focus to the terminal"
        );
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.worktrees", None).unwrap();
        let dialog = app.window.visible_dialog().unwrap();
        wait_ui(|| has_label(&dialog.child().unwrap(), "/tmp/main <checkout>&"));
        assert!(has_label(
            &dialog.child().unwrap(),
            "/tmp/parallel agent/feature <checkout>&"
        ));
        capture_workflow(&app.window, &format!("worktrees-{theme}"));
        dialog.close();
        wait_ui(|| app.window.visible_dialog().is_none());
        app.show_workspace("a");
    }
    app.window.set_default_size(360, 680);
    wait_ui(|| app.window.width() <= 400);
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.worktrees", None).unwrap();
    let narrow = app.window.visible_dialog().unwrap();
    wait_ui(|| {
        has_label(
            &narrow.child().unwrap(),
            "/tmp/parallel agent/feature <checkout>&",
        )
    });
    capture_workflow(&app.window, "worktrees-narrow");
    narrow.close();
    wait_ui(|| app.window.visible_dialog().is_none());
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-changes", None).unwrap();
    // Narrow windows overlay the panel rather than squeezing panes.
    wait_ui(|| app.changes_split.is_collapsed());
    let panel = app.changes.widget.clone().upcast::<gtk4::Widget>();
    wait_ui(|| has_label(&panel, "Binary"));
    wait_ui(|| panel.width() > 0 && panel.width() <= app.changes_split.width());
    assert!(
        app.split_view.is_collapsed(),
        "the narrow breakpoint still wins"
    );
    capture_workflow(&app.window, "changes-narrow");
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-changes", None).unwrap();
    assert!(!app.changes_split.shows_sidebar());
    app.window.set_default_size(920, 680);
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.worktrees", None).unwrap();
    let worktrees = app.window.visible_dialog().unwrap();
    wait_ui(|| {
        has_label(
            &worktrees.child().unwrap(),
            "/tmp/parallel agent/feature <checkout>&",
        )
    });
    let row = action_row_with_title(
        &worktrees.child().unwrap(),
        "/tmp/parallel agent/feature <checkout>&",
    )
    .unwrap();
    button_with_label(row.upcast_ref(), "Remove…")
        .unwrap()
        .emit_clicked();
    let confirmation = app
        .window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    respond(&app, &confirmation, "remove");
    wait_ui(|| {
        has_label(
            &worktrees.child().unwrap(),
            "WORKTREE_DIRTY: checkout has local changes",
        )
    });
    assert!(has_label(
        &worktrees.child().unwrap(),
        "/tmp/parallel agent/feature <checkout>&"
    ));
    button_with_label(row.upcast_ref(), "Open")
        .unwrap()
        .emit_clicked();
    wait_ui(|| app.window.visible_dialog().is_none() && app.active_ws_id().as_deref() == Some("b"));
    drain_refresh(&app);
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.worktrees", None).unwrap();
    let worktrees = app.window.visible_dialog().unwrap();
    wait_ui(|| has_label(&worktrees.child().unwrap(), "/tmp/main <checkout>&"));
    button_with_label(&worktrees.child().unwrap(), "Create Worktree…")
        .unwrap()
        .emit_clicked();
    let creation = app
        .window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    let mut entries = Vec::new();
    collect_entries(&creation.extra_child().unwrap(), &mut entries);
    entries[0].set_text("/tmp/parallel <new>&");
    entries[1].set_text("feature/new");
    assert!(creation.is_response_enabled("create"));
    respond(&app, &creation, "create");
    wait_ui(|| app.active_ws_id().as_deref() == Some("c"));
    drain_refresh(&app);
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|(method, params)| method == "pane.spawn" && params["workspace_id"] == "c"));
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|(method, params)| method == "worktree.create"
            && params["path"] == "/tmp/parallel <new>&"
            && params["branch"] == "feature/new"));
    app.show_workspace("a");
    assert_eq!(app.widgets.borrow()["pane_a"].root, original);
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.rename-workspace", None).unwrap();
    let dialog = app
        .window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    let entry = find_widget::<adw::EntryRow>(&dialog.extra_child().unwrap()).unwrap();
    entry.set_text("");
    assert!(!dialog.is_response_enabled("rename"));
    entry.set_text("Workspace <renamed> & ready");
    respond(&app, &dialog, "rename");
    wait_ui(|| state.lock().unwrap()["a"]["workspace"]["name"] == "Workspace <renamed> & ready");
    assert!(calls
        .lock()
        .unwrap()
        .iter()
        .any(|(method, params)| method == "workspace.rename" && params["workspace_id"] == "a"));
    style.set_color_scheme(original_scheme);
    settings.set_gtk_enable_animations(original_motion);
    let stop = app.actor.clone();
    glib::MainContext::default()
        .block_on(stop.call("test.stop", json!({})))
        .unwrap();
    worker.join().unwrap();
    app.window.destroy();
}

fn next_diff_call(
    requests: &mut tokio::sync::mpsc::UnboundedReceiver<ActorRequest>,
    expected_method: &str,
) -> (Value, tokio::sync::oneshot::Sender<Result<Value, String>>) {
    wait_ui(|| !requests.is_empty());
    let ActorRequest::Call {
        method,
        params,
        reply,
        ..
    } = requests.try_recv().unwrap()
    else {
        panic!("expected call")
    };
    assert_eq!(method, expected_method);
    (params, reply)
}

fn numbered_diff_fixture(path: &str) -> Value {
    json!({"path":path,"untracked":false,"content":{
        "kind":"text","truncated":false,"notice":null,"hunks":[{
            "old_start":1,"old_count":2,"new_start":1,"new_count":3,"heading":"@@ -1,2 +1,3 @@",
            "lines":[
                {"kind":"context","text":"let literal = \"<>&\";","old_line":1,"new_line":1},
                {"kind":"removed","text":"old","old_line":2,"new_line":null},
                {"kind":"added","text":"new","old_line":null,"new_line":2},
                {"kind":"added","text":"extra","old_line":null,"new_line":3}
            ]
        },{
            "old_start":20,"old_count":1,"new_start":21,"new_count":1,"heading":"@@ -20 +21 @@ second hunk",
            "lines":[
                {"kind":"removed","text":"before","old_line":20,"new_line":null},
                {"kind":"added","text":"after","old_line":null,"new_line":21}
            ]
        }]
    }})
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn changes_panel_scopes_to_the_latest_turn() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(920, 680);
    let terminal = gtk4::Label::new(Some("Workspace terminals stay mounted"));
    terminal.set_hexpand(true);
    let (actor, mut requests) = IpcHandle::test_channel();
    let panel = crate::changes::ChangesPanel::new(actor);
    let host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    host.append(&terminal);
    host.append(&panel.widget);
    window.set_content(Some(&host));
    window.present();
    panel.show("a");
    let root = panel.widget.clone().upcast::<gtk4::Widget>();
    let (params, reply) = next_diff_call(&mut requests, "workspace.diff");
    assert_eq!(params["scope"], "head");
    reply
        .send(Ok(json!({"added":1,"removed":0,"files":[
            {"path":"README.md","added":1,"removed":0,"binary":false,"untracked":false}
        ]})))
        .unwrap();
    wait_ui(|| has_label(&root, "Changes against HEAD · 1 files · +1 −0"));
    // The turn view does not follow events.
    panel.on_event("agent.done", Some("a"));
    assert!(requests.is_empty());

    let scopes = find_widget::<adw::ToggleGroup>(&root).unwrap();
    assert_eq!(scopes.n_toggles(), 2);
    scopes.set_active_name(Some("turn"));
    let (params, reply) = next_diff_call(&mut requests, "workspace.diff");
    assert_eq!(params["scope"], "turn");
    reply
        .send(Ok(json!({"added":0,"removed":0,"files":[],"turn":null})))
        .unwrap();
    wait_ui(|| {
        action_row_with_title(&root, "No agent turn recorded in this workspace yet.").is_some()
    });
    assert!(action_row_with_title(&root, "README.md").is_none());

    // Only this workspace's turns re-read the list.
    panel.on_event("agent.done", Some("b"));
    panel.on_event("agent.working", Some("a"));
    assert!(requests.is_empty());
    panel.on_event("workspace.turn_started", Some("a"));
    let (params, reply) = next_diff_call(&mut requests, "workspace.diff");
    assert_eq!(params["scope"], "turn");
    reply
        .send(Ok(json!({"added":3,"removed":1,
        "turn":{"pane_id":"pane_a","started_at":"2026-10-10T10:00:00Z"},
        "files":[
            {"path":"src/new.rs","added":2,"removed":0,"binary":false,"untracked":false},
            {"path":"notes.txt","added":1,"removed":1,"binary":false,"untracked":false}
        ]})))
        .unwrap();
    wait_ui(|| has_label(&root, "Latest turn · 2 files · +3 −1"));
    capture_workflow(&window, "changes-turn");

    let row = action_row_with_title(&root, "new.rs").unwrap();
    adw::prelude::ActionRowExt::activate(&row);
    let (params, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    assert_eq!(params["path"], "src/new.rs");
    assert_eq!(params["scope"], "turn");
    reply
        .send(Ok(
            json!({"path":"src/new.rs","untracked":false,"content":{"kind":"unchanged"}}),
        ))
        .unwrap();
    wait_ui(|| has_label(&root, "This file no longer has changes in the latest turn."));
    assert!(has_label(&root, "Changes in the latest turn"));
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn file_diff_reader_uses_native_numbered_selectable_controls() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(920, 680);
    let terminal = gtk4::Label::new(Some("Workspace terminals stay mounted"));
    terminal.set_hexpand(true);
    let (actor, mut requests) = IpcHandle::test_channel();
    let panel = crate::changes::ChangesPanel::new(actor.clone());
    let host = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    host.append(&terminal);
    host.append(&panel.widget);
    window.set_content(Some(&host));
    window.present();
    panel.show("a");
    let root = panel.widget.clone().upcast::<gtk4::Widget>();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    let summary = json!({"added":2,"removed":1,"files":[
        {"path":"src/<changed>&.rs","added":2,"removed":1,"binary":false,"untracked":false},
        {"path":"notes/new <file>&.txt","added":0,"removed":0,"binary":false,"untracked":true},
        {"path":"assets/banner.png","added":0,"removed":0,"binary":true,"untracked":false}
    ]});
    reply.send(Ok(summary.clone())).unwrap();
    wait_ui(|| has_label(&root, "<changed>&.rs"));
    assert!(
        requests.is_empty(),
        "opening summary must not automatically read a file"
    );
    let row = action_row_with_title(&root, "<changed>&.rs").unwrap();
    let rows = find_widget::<gtk4::ListBox>(&root).unwrap();
    let list_scroll = rows
        .parent()
        .unwrap()
        .downcast::<gtk4::Viewport>()
        .unwrap()
        .parent()
        .unwrap()
        .downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    assert!(row.is_activatable());
    rows.select_row(Some(&row));
    assert!(row.grab_focus());
    // This is the native GTK action bound to Enter on the focused list row.
    rows.emit_activate_cursor_row();
    let (params, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    assert_eq!(params["path"], "src/<changed>&.rs");
    reply
        .send(Ok(numbered_diff_fixture("src/<changed>&.rs")))
        .unwrap();
    let reader = find_widget::<gtk4::TextView>(&root).unwrap();
    let reader_text = || {
        reader
            .buffer()
            .text(
                &reader.buffer().start_iter(),
                &reader.buffer().end_iter(),
                false,
            )
            .to_string()
    };
    wait_ui(|| reader_text().contains("after"));
    assert!(!reader.is_editable());
    assert!(reader.is_monospace());
    assert!(reader.is_cursor_visible());
    let text = reader_text();
    assert!(text.contains(" 1   1    let literal = \"<>&\";"), "{text}");
    assert!(text.contains(" 2      − old"), "{text}");
    assert!(text.contains("     2  + new"), "{text}");
    assert!(text.contains("20      − before"), "{text}");
    assert!(text.contains("    21  + after"), "{text}");
    assert!(has_label(&root, "Changes against HEAD"));
    let buffer = reader.buffer();
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    reader.emit_copy_clipboard();
    let copied = Rc::new(RefCell::new(None));
    let capture = copied.clone();
    reader
        .clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |result| {
            *capture.borrow_mut() = Some(result.unwrap().unwrap().to_string())
        });
    wait_ui(|| copied.borrow().is_some());
    assert_eq!(copied.borrow().as_ref().unwrap(), &text);
    buffer.place_cursor(&buffer.start_iter());
    let style = adw::StyleManager::default();
    let mut palettes = Vec::new();
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        style.set_color_scheme(scheme);
        if let Some(previous) = palettes.last() {
            wait_ui(|| {
                buffer
                    .tag_table()
                    .lookup("added")
                    .unwrap()
                    .foreground_rgba()
                    .as_ref()
                    != Some(previous)
            });
        }
        capture_workflow(&window, &format!("file-diff-{theme}"));
        let added = buffer
            .tag_table()
            .lookup("added")
            .unwrap()
            .foreground_rgba()
            .unwrap();
        let removed = buffer
            .tag_table()
            .lookup("removed")
            .unwrap()
            .foreground_rgba()
            .unwrap();
        assert_ne!(
            added, removed,
            "addition/removal colors must be distinct in {theme}"
        );
        assert_ne!(
            added,
            reader.color(),
            "semantic foreground must be applied in {theme}"
        );
        palettes.push(added);
    }
    assert_ne!(
        palettes[0], palettes[1],
        "semantic diff colors must adapt to light and dark themes"
    );
    let reader_scroll = reader
        .parent()
        .unwrap()
        .downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    let scroll = list_scroll.vadjustment().value();
    let navigation = find_widget::<adw::NavigationView>(&root).unwrap();
    let back = button_with_tooltip(
        &navigation.visible_page().unwrap().child().unwrap(),
        "Return to changed files (Alt+Left)",
    )
    .unwrap();
    back.emit_clicked();
    assert_eq!(
        rows.selected_row().unwrap(),
        row.clone().upcast::<gtk4::ListBoxRow>()
    );
    assert_eq!(
        gtk4::prelude::GtkWindowExt::focus(&window).unwrap(),
        row.clone().upcast::<gtk4::Widget>()
    );
    assert_eq!(list_scroll.vadjustment().value(), scroll);
    assert!(requests.is_empty());
    // The list and actual row survive refresh; only explicit activation reads.
    let list_refresh = button_with_tooltip(
        &navigation.visible_page().unwrap().child().unwrap(),
        "Refresh working tree changes",
    )
    .unwrap();
    list_refresh.emit_clicked();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply.send(Ok(summary.clone())).unwrap();
    wait_ui(|| has_label(&root, "Changes against HEAD · 3 files · +2 −1"));
    assert_eq!(action_row_with_title(&root, "<changed>&.rs").unwrap(), row);
    assert!(requests.is_empty());
    // A pending read cannot overwrite B after Back and a new activation.
    row.emit_by_name::<()>("activated", &[]);
    let (_, old_reply) = next_diff_call(&mut requests, "workspace.file_diff");
    back.emit_clicked();
    let new_row = action_row_with_title(&root, "new <file>&.txt").unwrap();
    new_row.emit_by_name::<()>("activated", &[]);
    let (params, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    assert_eq!(params["path"], "notes/new <file>&.txt");
    let new_diff = json!({"path":"notes/new <file>&.txt","untracked":true,"content":{
        "kind":"text","truncated":true,"notice":"Preview truncated at 10,000 lines.","hunks":[{
            "old_start":0,"old_count":0,"new_start":1,"new_count":10001,"heading":"@@ -0,0 +1,10001 @@",
            "lines":[
                {"kind":"added","text":"New content <>&","old_line":null,"new_line":1},
                {"kind":"added","text":"More new content","old_line":null,"new_line":2}
            ]
        }]
    }});
    reply.send(Ok(new_diff)).unwrap();
    wait_ui(|| has_label(&root, "Untracked · Preview truncated at 10,000 lines."));
    old_reply
        .send(Ok(
            json!({"path":"src/<changed>&.rs","untracked":false,"content":{"kind":"binary"}}),
        ))
        .unwrap();
    while glib::MainContext::default().iteration(false) {}
    assert!(reader_text().contains("New content <>&"));
    assert!(!has_label(
        &root,
        "Binary file changed. No text preview is available."
    ));
    capture_workflow(&window, "file-diff-incomplete");
    // Refresh invalidates the displayed patch and its pending response.
    new_row.emit_by_name::<()>("activated", &[]);
    let (_, stale_refresh) = next_diff_call(&mut requests, "workspace.file_diff");
    let refresh = button_with_tooltip(
        &navigation.visible_page().unwrap().child().unwrap(),
        "Refresh working tree changes",
    )
    .unwrap();
    refresh.emit_clicked();
    assert!(reader_text().is_empty());
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply.send(Ok(summary.clone())).unwrap();
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply.send(Ok(json!({"path":"notes/new <file>&.txt","untracked":true,"content":{"kind":"text","hunks":[],"truncated":false,"notice":"Empty untracked file."}}))).unwrap();
    wait_ui(|| has_label(&root, "Untracked · Empty untracked file."));
    stale_refresh
        .send(Ok(numbered_diff_fixture("notes/new <file>&.txt")))
        .unwrap();
    while glib::MainContext::default().iteration(false) {}
    assert!(reader_text().is_empty());
    new_row.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply
        .send(Err("IO_ERROR: unreadable <file>&".into()))
        .unwrap();
    wait_ui(|| {
        has_label(
            &root,
            "Couldn't read this file: IO_ERROR: unreadable <file>&. Refresh to try again.",
        )
    });
    // Pending Back response is ignored while the list remains visible.
    new_row.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    back.emit_clicked();
    reply
        .send(Ok(numbered_diff_fixture("notes/new <file>&.txt")))
        .unwrap();
    while glib::MainContext::default().iteration(false) {}
    assert!(reader_text().is_empty());
    let binary = action_row_with_title(&root, "banner.png").unwrap();
    binary.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply
        .send(Ok(
            json!({"path":"assets/banner.png","untracked":false,"content":{"kind":"binary"}}),
        ))
        .unwrap();
    wait_ui(|| has_label(&root, "Binary file changed. No text preview is available."));
    capture_workflow(&window, "file-diff-binary");
    for (content, notice) in [
        (
            json!({"kind":"unchanged"}),
            "This file no longer has changes against HEAD.",
        ),
        (
            json!({"kind":"unavailable","reason":"Unsupported <file>& content."}),
            "Unsupported <file>& content.",
        ),
    ] {
        binary.emit_by_name::<()>("activated", &[]);
        let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
        reply
            .send(Ok(
                json!({"path":"assets/banner.png","untracked":false,"content":content}),
            ))
            .unwrap();
        wait_ui(|| has_label(&root, notice));
        assert!(reader_text().is_empty());
    }

    row.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply
        .send(Ok(numbered_diff_fixture("src/<changed>&.rs")))
        .unwrap();
    wait_ui(|| reader_text().contains("after"));
    // Narrow windows overlay the panel over the terminals (see the
    // navigation test); here the panel gets the whole width.
    terminal.set_visible(false);
    window.set_default_size(360, 680);
    wait_ui(|| window.width() <= 400);
    wait_ui(|| reader_scroll.width() <= 360);
    assert!(back.width() > 0 && back.width() < 100);
    capture_workflow(&window, "file-diff-narrow");
    assert_eq!(
        terminal.parent().unwrap(),
        host.clone().upcast::<gtk4::Widget>()
    );
    // Switching workspace drops the old rows and any read still in flight.
    row.emit_by_name::<()>("activated", &[]);
    let (_, stale) = next_diff_call(&mut requests, "workspace.file_diff");
    panel.show("b");
    let (params, reply) = next_diff_call(&mut requests, "workspace.diff");
    assert_eq!(params["workspace_id"], "b");
    reply
        .send(Ok(json!({"added":0,"removed":0,"files":[]})))
        .unwrap();
    stale
        .send(Ok(numbered_diff_fixture("src/<changed>&.rs")))
        .unwrap();
    wait_ui(|| has_label(&root, "No working tree changes"));
    assert!(!has_label(&root, "<changed>&.rs"));
    assert!(reader_text().is_empty());
    assert_eq!(
        navigation.visible_page().unwrap().title(),
        "Changes",
        "a new workspace starts at its file list"
    );
    button_with_tooltip(&root, "Refresh working tree changes")
        .unwrap()
        .emit_clicked();
    let (params, reply) = next_diff_call(&mut requests, "workspace.diff");
    assert_eq!(params["workspace_id"], "b");
    reply
        .send(Ok(json!({"added":0,"removed":0,"files":[
            {"path":"-option","added":0,"removed":0,"binary":false,"untracked":true}
        ]})))
        .unwrap();
    wait_ui(|| has_label(&root, "-option"));
    assert!(!has_label(&root, "No working tree changes"));
    window.destroy();
    style.set_color_scheme(adw::ColorScheme::Default);
}

/// Worker chip sits in the pane title cluster, hugs its text, and drops
/// a label that repeats the workspace name. Screenshots land in
/// `$SIGNALTTY_UI_EVIDENCE` when that directory is set.
#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn worker_chip_sits_with_the_pane_title() {
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    let config = std::env::temp_dir().join(format!("signaltty-chip-scene-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    let contrast = std::env::var("ADW_DEBUG_HIGH_CONTRAST").ok().as_deref() == Some("1");

    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let display = gtk4::gdk::Display::default().unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk4::IconTheme::for_display(&display).add_resource_path("/dev/signaltty/gui/icons");
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let snapshot = chip_scene_snapshot();
    {
        let mut model = app.model.borrow_mut();
        model.cache.workspaces = vec![snapshot.workspace.clone()];
        model
            .cache
            .snapshots
            .insert(snapshot.workspace.id.clone(), snapshot.clone());
        assert!(model.tasks.apply_event(
            signaltty_proto::event::TASK_CREATED,
            &json!({"task": chip_scene_task("fix-parser")}),
        ));
    }
    app.show_workspace("fix");
    app.sidebar.update(vec![crate::sidebar::summarize(
        &snapshot.workspace,
        &snapshot.panes,
    )]);
    app.paint_task_chips();
    app.widgets.borrow()["pane_fix"].feed(b"parser worker\r\n");
    app.window.set_default_size(1280, 800);
    app.present();
    let window = app.window.clone().upcast::<gtk4::Widget>();
    wait_ui(|| {
        try_header(&window, "worker").is_some_and(|header| {
            header.width() > 400
                && try_descendant(&header, "task-chip").is_some_and(|chip| chip.width() > 20)
        })
    });

    let paint = |appearance, theme: signaltty_core::theme::Theme, name: &str| {
        app.set_theme(theme);
        app.set_appearance(appearance);
        let window = app.window.clone().upcast::<gtk4::Widget>();
        wait_ui(|| app.window.width() > 1000);
        assert_chip_cluster(&window);
        capture_workflow(&app.window, name);
    };
    if contrast {
        app.set_theme(signaltty_core::theme::Theme::Signal);
        app.set_appearance(signaltty_core::theme::Appearance::Dark);
        wait_ui(|| app.window.has_css_class("high-contrast"));
        paint(
            signaltty_core::theme::Appearance::Dark,
            signaltty_core::theme::Theme::Signal,
            "signal-dark-high-contrast",
        );
        app.window.destroy();
        match previous_config {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
        return;
    }
    paint(
        signaltty_core::theme::Appearance::Light,
        signaltty_core::theme::Theme::Signal,
        "signal-light",
    );
    paint(
        signaltty_core::theme::Appearance::Dark,
        signaltty_core::theme::Theme::Signal,
        "signal-dark",
    );
    paint(
        signaltty_core::theme::Appearance::Dark,
        signaltty_core::theme::Theme::Grove,
        "grove-dark",
    );
    app.set_theme(signaltty_core::theme::Theme::Signal);
    app.set_appearance(signaltty_core::theme::Appearance::Dark);

    let mut split = snapshot.clone();
    let mut sibling = split.panes[0].clone();
    sibling.id = "pane_shell".into();
    sibling.title = "notes".into();
    sibling.last_message = None;
    split.panes.push(sibling);
    split.tabs[0].layout = Some(signaltty_core::Layout::Split {
        dir: signaltty_core::SplitDir::Right,
        ratio: 0.42,
        first: Box::new(signaltty_core::Layout::Pane {
            pane_id: "pane_fix".into(),
        }),
        second: Box::new(signaltty_core::Layout::Pane {
            pane_id: "pane_shell".into(),
        }),
    });
    {
        let mut model = app.model.borrow_mut();
        model.cache.workspaces = vec![split.workspace.clone()];
        model
            .cache
            .snapshots
            .insert(split.workspace.id.clone(), split.clone());
    }
    app.show_workspace("fix");
    app.sidebar.update(vec![crate::sidebar::summarize(
        &split.workspace,
        &split.panes,
    )]);
    app.paint_task_chips();
    let window = app.window.clone().upcast::<gtk4::Widget>();
    wait_ui(|| try_pane(&window, "worker").is_some_and(|pane| pane.width() > 100));
    for _ in 0..4 {
        let width = try_pane(&window, "worker").unwrap().width();
        if (360..=440).contains(&width) {
            break;
        }
        let paned = find_widget::<gtk4::Paned>(&window).unwrap();
        // Production panes keep their natural width. This shot needs a
        // ~400px worker, so the test divider is allowed to shrink.
        paned.set_shrink_start_child(true);
        paned.set_shrink_end_child(true);
        let total = paned.width().max(1) as f32;
        let ratio = (400.0 / total).clamp(0.2, 0.8);
        {
            let mut model = app.model.borrow_mut();
            if let Some(signaltty_core::Layout::Split { ratio: slot, .. }) =
                model.tabs[0].layout.as_mut()
            {
                *slot = ratio;
            }
            if let Some(signaltty_core::Layout::Split { ratio: slot, .. }) = model
                .cache
                .snapshots
                .get_mut("fix")
                .and_then(|snap| snap.tabs[0].layout.as_mut())
            {
                *slot = ratio;
            }
        }
        app.render_tabs();
        let paned = find_widget::<gtk4::Paned>(&window).unwrap();
        paned.set_shrink_start_child(true);
        paned.set_shrink_end_child(true);
        app.dividers.suppressing(|| paned.set_position(400));
        let deadline = Instant::now() + std::time::Duration::from_millis(400);
        while Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    let pane_width = try_pane(&window, "worker").unwrap().width();
    let paned = find_widget::<gtk4::Paned>(&window).unwrap();
    assert!(
        (360..=440).contains(&pane_width),
        "worker pane is {pane_width}px, wanted about 400 (paned {} pos {})",
        paned.width(),
        paned.position()
    );
    assert_chip_cluster(&window);
    assert!(
        !label_with_class(&try_header(&window, "worker").unwrap(), "pane-title")
            .layout()
            .is_ellipsized(),
        "pane title ellipsized in a narrow pane"
    );
    capture_workflow(&app.window, "signal-dark-narrow");

    assert!(app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": chip_scene_task("parser-recovery")}),
    ));
    app.paint_task_chips();
    let deadline = Instant::now() + std::time::Duration::from_millis(200);
    while Instant::now() < deadline {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let header = try_header(&window, "worker").unwrap();
    let title = label_with_class(&header, "pane-title");
    let chip_label = label_with_class(&header, "task-chip-label");
    assert!(
        !title.layout().is_ellipsized(),
        "title ellipsized before the task label"
    );
    assert!(
        chip_label.layout().is_ellipsized(),
        "task label did not ellipsize in a {}px pane (label width {})",
        try_pane(&window, "worker").unwrap().width(),
        chip_label.width()
    );
    let chip = try_descendant(&header, "task-chip").unwrap();
    let actions = try_descendant(&header, "pane-actions").unwrap();
    let chip_box = widget_bounds(&chip, &header);
    let actions_box = widget_bounds(&actions, &header);
    assert!(
        chip_box.x + chip_box.w <= actions_box.x + 1.0,
        "chip overlaps the header actions"
    );

    app.window.destroy();
    match previous_config {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn task_board_shows_columns_and_navigates_to_pane() {
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    let config = std::env::temp_dir().join(format!("signaltty-board-scene-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("SIGNALTTY_NOTIFY", "0");

    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let display = gtk4::gdk::Display::default().unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk4::IconTheme::for_display(&display).add_resource_path("/dev/signaltty/gui/icons");
    let (actor, mut requests) = IpcHandle::test_channel();
    let listed_tasks = std::sync::Arc::new(std::sync::Mutex::new(json!({})));
    let worker_tasks = listed_tasks.clone();
    // Card activation focuses through `pane.get`; answer it from workspace b.
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call { method, reply, .. } = request {
                let result = match method.as_str() {
                    "task.list" => Ok(worker_tasks.lock().unwrap().clone()),
                    "pane.get" => Ok(json!({"pane": fixture("b")["panes"][0]})),
                    "test.stop" => {
                        let _ = reply.send(Ok(Value::Null));
                        break;
                    }
                    _ => Ok(json!({})),
                };
                let _ = reply.send(result);
            }
        }
    });
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    let snapshot = chip_scene_snapshot();
    let other: crate::refresh::Snapshot = serde_json::from_value(fixture("b")).unwrap();
    {
        let mut model = app.model.borrow_mut();
        model.cache.workspaces = vec![snapshot.workspace.clone(), other.workspace.clone()];
        for snap in [&snapshot, &other] {
            model
                .cache
                .snapshots
                .insert(snap.workspace.id.clone(), snap.clone());
        }
    }
    app.show_workspace("fix");
    app.window.set_default_size(1280, 800);
    app.present();

    // 1. Test empty state
    gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-board", None).unwrap();
    let dialog = app.window.visible_dialog().unwrap();
    wait_ui(|| has_label(&dialog.child().unwrap(), "No Tasks Yet"));
    capture_workflow(&app.window, "board-empty");
    app.window.visible_dialog().unwrap().close();
    wait_ui(|| app.window.visible_dialog().is_none());

    // 2. Populate 4 tasks in the model
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();
    let older_str = (now - chrono::Duration::minutes(5)).to_rfc3339();
    {
        let mut model = app.model.borrow_mut();
        let tasks = vec![
            json!({
                "id": "task_working_1",
                "context_id": "ctx1",
                "pane_id": "pane_b",
                "label": "Implement Task Board",
                "contract": {"objective": "Build task board view"},
                "agent": "codex",
                "source_repo": "/tmp/repo",
                "worktree_path": "/tmp/wt1",
                "branch": "orch/board",
                "base_ref": "main",
                "base_sha": "0123456789abcdef",
                "state": "working",
                "disposition": {"outcome": "none"},
                "created_at": older_str,
                "updated_at": now_str,
            }),
            json!({
                "id": "task_needs_you_1",
                "context_id": "ctx2",
                "pane_id": "pane_fix_2",
                "label": "Permission required for network",
                "contract": {"objective": "Fetch upstream assets"},
                "agent": "claude",
                "source_repo": "/tmp/repo",
                "worktree_path": "/tmp/wt2",
                "branch": "orch/assets",
                "base_ref": "main",
                "base_sha": "0123456789abcdef",
                "state": "input_required",
                "disposition": {"outcome": "none"},
                "created_at": older_str,
                "updated_at": now_str,
            }),
            json!({
                "id": "task_in_review_1",
                "context_id": "ctx3",
                "pane_id": "pane_fix_3",
                "label": "Add unit tests for CLI",
                "contract": {"objective": "Write unit tests"},
                "agent": "opencode",
                "source_repo": "/tmp/repo",
                "worktree_path": "/tmp/wt3",
                "branch": "orch/tests",
                "base_ref": "main",
                "base_sha": "0123456789abcdef",
                "state": "completed",
                "disposition": {"outcome": "none"},
                "created_at": older_str,
                "updated_at": now_str,
            }),
            json!({
                "id": "task_done_1",
                "context_id": "ctx4",
                "pane_id": "pane_fix_4",
                "label": "Initial multiplexer core",
                "contract": {"objective": "Multiplexer"},
                "agent": "codex",
                "source_repo": "/tmp/repo",
                "worktree_path": "/tmp/wt4",
                "branch": "orch/core",
                "base_ref": "main",
                "base_sha": "0123456789abcdef",
                "state": "completed",
                "disposition": {"outcome": "merged"},
                "created_at": older_str,
                "updated_at": older_str,
            }),
        ];

        for t in &tasks {
            assert!(model
                .tasks
                .apply_event(signaltty_proto::event::TASK_CREATED, &json!({"task": t}),));
        }
    }

    // An event must rebuild an open board, without another PR request or
    // allowing the replaced dialog's close callback to clear the new one.
    app.action_show_board();
    let mut changed = app.model.borrow().tasks.iter().next().unwrap().clone();
    let original = changed.clone();
    changed.label = "Updated while board is open".into();
    app.on_event(UiEvent::ServerEvent {
        name: signaltty_proto::event::TASK_UPDATED.into(),
        payload: json!({"task": changed}),
    });
    wait_ui(|| {
        app.window.visible_dialog().is_some_and(|dialog| {
            has_label(&dialog.child().unwrap(), "Updated while board is open")
        })
    });
    let dialog = app.window.visible_dialog().unwrap();
    assert_eq!(
        app.board_dialog
            .borrow()
            .as_ref()
            .map(|board| &board.dialog),
        Some(&dialog)
    );
    app.window.visible_dialog().unwrap().close();
    wait_ui(|| app.window.visible_dialog().is_none());
    app.refresh_open_board();
    assert!(app.window.visible_dialog().is_none());
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": original}),
    );

    let style = adw::StyleManager::default();
    for (theme, scheme) in [
        ("dark", adw::ColorScheme::ForceDark),
        ("light", adw::ColorScheme::ForceLight),
    ] {
        style.set_color_scheme(scheme);
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-board", None).unwrap();
        let dialog = app.window.visible_dialog().unwrap();
        wait_ui(|| has_board_column_count(&dialog.child().unwrap(), "Working", "1"));
        assert!(has_board_column_count(
            &dialog.child().unwrap(),
            "Needs you",
            "1"
        ));
        assert!(has_board_column_count(
            &dialog.child().unwrap(),
            "In review",
            "1"
        ));
        assert!(has_board_column_count(
            &dialog.child().unwrap(),
            "Done",
            "1"
        ));
        assert!(has_label(&dialog.child().unwrap(), "Implement Task Board"));
        assert!(has_label(
            &dialog.child().unwrap(),
            "Permission required for network"
        ));
        assert!(has_label(
            &dialog.child().unwrap(),
            "Add unit tests for CLI"
        ));
        assert!(has_label(
            &dialog.child().unwrap(),
            "Initial multiplexer core"
        ));

        capture_workflow(&app.window, &format!("board-{theme}"));
        let dialog = app.window.visible_dialog().unwrap();
        if theme == "dark" {
            app.window.visible_dialog().unwrap().close();
            wait_ui(|| app.window.visible_dialog().is_none());
        } else {
            // The first card is the Working task in workspace b.
            assert_eq!(app.current_pane_id().as_deref(), Some("pane_fix"));
            let row = find_widget::<gtk4::ListBoxRow>(&dialog.child().unwrap()).unwrap();
            row.activate();
            wait_ui(|| app.window.visible_dialog().is_none());
            wait_ui(|| app.current_pane_id().as_deref() == Some("pane_b"));
        }
    }

    // Refresh completion must reload tasks even without a task.updated event.
    let mut refreshed = app.model.borrow().tasks.iter().next().unwrap().clone();
    refreshed.label = "Reloaded after PR refresh".into();
    *listed_tasks.lock().unwrap() = json!({"tasks": [refreshed]});
    app.action_show_board();
    wait_ui(|| {
        app.window
            .visible_dialog()
            .is_some_and(|dialog| has_label(&dialog.child().unwrap(), "Reloaded after PR refresh"))
    });
    app.window.visible_dialog().unwrap().close();
    wait_ui(|| app.window.visible_dialog().is_none());

    // A completion arriving after dismissal must leave the board closed.
    listed_tasks.lock().unwrap()["tasks"][0]["label"] = json!("Refreshed after dismissal");
    app.action_show_board();
    app.window.visible_dialog().unwrap().close();
    wait_ui(|| {
        app.model
            .borrow()
            .tasks
            .iter()
            .any(|task| task.label == "Refreshed after dismissal")
    });
    assert!(app.board_dialog.borrow().is_none());
    assert!(app.window.visible_dialog().is_none());

    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    worker.join().unwrap();
    app.window.destroy();
    match previous_config {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn task_board_fits_narrow_windows_and_reveals_last_column() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let display = gtk4::gdk::Display::default().unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let settings = gtk4::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    let old_font = settings.gtk_font_name();
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(360, 600);
    window.present();
    let mut task = chip_scene_task("A long task title with <literal> symbols & branch context");
    task["pane_id"] = json!("pane_done");
    task["state"] = json!("completed");
    task["disposition"] = json!({"outcome": "merged"});
    task["agent"] = json!("codex");
    task["branch"] = json!("fix/a-long-branch-name-with-review-context");
    let task: signaltty_core::Task = serde_json::from_value(task).unwrap();
    let mut working = task.clone();
    working.id = "task_working".into();
    working.pane_id = Some("pane_working".into());
    working.state = signaltty_core::TaskState::Working;
    working.disposition.outcome = signaltty_core::DispositionOutcome::None;
    let tasks = [working, task];
    let chosen = Rc::new(RefCell::new(None));
    for (name, scheme, high_contrast) in [
        ("light", adw::ColorScheme::ForceLight, false),
        ("dark", adw::ColorScheme::ForceDark, false),
        ("large-high-contrast", adw::ColorScheme::ForceLight, true),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        if scheme == adw::ColorScheme::ForceDark {
            window.add_css_class("dark");
        } else {
            window.remove_css_class("dark");
        }
        if high_contrast {
            window.add_css_class("high-contrast");
            settings.set_gtk_font_name(Some("Sans 18"));
        }
        let result = chosen.clone();
        let board = crate::board::present(&window, &tasks, move |_, pane| {
            result.replace(pane);
        });
        let dialog = &board.dialog;
        wait_ui(|| dialog.width() > 0);
        capture_workflow(&window, &format!("board-narrow-{name}-start"));
        assert!(window.width() <= 360, "board must not widen its parent");
        assert!(
            dialog.width() <= window.width(),
            "dialog width {} exceeds parent",
            dialog.width()
        );
        let root = dialog.child().unwrap();
        let mut horizontal = None;
        walk_widgets(&root, &mut |widget| {
            if let Some(scroll) = widget.downcast_ref::<gtk4::ScrolledWindow>() {
                if scroll.hadjustment().upper() > scroll.hadjustment().page_size() {
                    horizontal = Some(scroll.clone());
                    return true;
                }
            }
            false
        });
        let scroll = horizontal.expect("overflowing columns must be scrollable");
        let adjustment = scroll.hadjustment();
        adjustment.set_value(adjustment.upper() - adjustment.page_size());
        let done = try_descendant(&root, "board-column-done").unwrap();
        let row = find_widget::<gtk4::ListBoxRow>(&done).unwrap();
        wait_ui(|| {
            row.compute_bounds(&scroll).is_some_and(|bounds| {
                bounds.x() >= 0.0 && bounds.x() + bounds.width() <= scroll.width() as f32
            })
        });
        capture_workflow(&window, &format!("board-narrow-{name}-done"));
        adjustment.set_value(0.0);
        wait_ui(|| {
            row.compute_bounds(&scroll)
                .is_some_and(|bounds| bounds.x() > scroll.width() as f32)
        });
        gtk4::prelude::GtkWindowExt::set_focus(&window, None::<&gtk4::Widget>);
        assert!(row.grab_focus());
        wait_ui(|| adjustment.value() > 0.0);
        row.activate();
        wait_ui(|| window.visible_dialog().is_none());
        assert_eq!(chosen.borrow().as_deref(), Some("pane_done"));
        chosen.replace(None);
    }
    settings.set_gtk_font_name(old_font.as_deref());
    window.destroy();
}

fn chip_scene_snapshot() -> crate::refresh::Snapshot {
    let now = chrono::Utc::now().to_rfc3339();
    let mut value = fixture("fix");
    value["workspace"]["name"] = json!("fix-parser");
    value["workspace"]["git"] = json!({"branch": "fix/parser"});
    value["workspace"]["created_at"] = json!(now);
    value["workspace"]["updated_at"] = json!(now);
    value["panes"][0]["title"] = json!("worker");
    value["panes"][0]["last_message"] = json!("session started");
    value["panes"][0]["lifecycle"] = json!("idle");
    value["panes"][0]["cwd"] = json!("/tmp/fix/parser");
    value["panes"][0]["created_at"] = json!(now);
    value["panes"][0]["last_activity_at"] = json!(now);
    serde_json::from_value(value).unwrap()
}

fn chip_scene_task(label: &str) -> Value {
    let now = chrono::Utc::now().to_rfc3339();
    json!({
        "id": "task_fix",
        "context_id": "tctx_fix",
        "pane_id": "pane_fix",
        "label": label,
        "contract": {"objective": "Tighten the parser"},
        "source_repo": "/tmp/repo",
        "worktree_path": "/tmp/wt",
        "branch": "fix/parser",
        "base_ref": "HEAD",
        "base_sha": "0123456789abcdef",
        "state": "working",
        "created_at": now,
        "updated_at": now
    })
}

struct WidgetBounds {
    x: f64,
    w: f64,
}

fn widget_bounds(widget: &gtk4::Widget, origin: &gtk4::Widget) -> WidgetBounds {
    let point = widget
        .compute_point(origin, &gtk4::graphene::Point::new(0.0, 0.0))
        .expect("widget coordinates");
    WidgetBounds {
        x: f64::from(point.x()),
        w: widget.width() as f64,
    }
}

fn walk_widgets(root: &gtk4::Widget, visit: &mut dyn FnMut(&gtk4::Widget) -> bool) -> bool {
    if visit(root) {
        return true;
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if walk_widgets(&widget, visit) {
            return true;
        }
        child = widget.next_sibling();
    }
    false
}

fn try_descendant(root: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
    let mut found = None;
    walk_widgets(root, &mut |widget| {
        if widget.has_css_class(class) {
            found = Some(widget.clone());
            true
        } else {
            false
        }
    });
    found
}

fn visible_label_texts(root: &gtk4::Widget) -> Vec<String> {
    let mut texts = Vec::new();
    walk_widgets(root, &mut |widget| {
        if !widget.is_visible() {
            return false;
        }
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            let text = label.text().to_string();
            if !text.is_empty() {
                texts.push(text);
            }
        }
        false
    });
    texts
}

fn try_header(window: &gtk4::Widget, title: &str) -> Option<gtk4::Widget> {
    let mut found = None;
    walk_widgets(window, &mut |widget| {
        if widget.has_css_class("pane-header")
            && visible_label_texts(widget).iter().any(|text| text == title)
        {
            found = Some(widget.clone());
            true
        } else {
            false
        }
    });
    found
}

fn try_pane(window: &gtk4::Widget, title: &str) -> Option<gtk4::Widget> {
    let mut found = None;
    walk_widgets(window, &mut |widget| {
        if widget.has_css_class("pane")
            && visible_label_texts(widget).iter().any(|text| text == title)
        {
            found = Some(widget.clone());
            true
        } else {
            false
        }
    });
    found
}

fn label_with_class(root: &gtk4::Widget, class: &str) -> gtk4::Label {
    let mut found = None;
    walk_widgets(root, &mut |widget| {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            if label.has_css_class(class) {
                found = Some(label.clone());
                return true;
            }
        }
        false
    });
    found.unwrap_or_else(|| panic!("missing label .{class}"))
}

/// Content width minus visible children. GTK's `width()` is the content
/// box, so CSS padding is outside this number and the remainder is the
/// box's inter-child spacing. A reserved label width shows up here.
fn chip_content_slack(chip: &gtk4::Widget) -> i32 {
    let mut content = 0;
    let mut child = chip.first_child();
    while let Some(widget) = child {
        if widget.is_visible() {
            content += widget.width();
        }
        child = widget.next_sibling();
    }
    chip.width() - content
}

/// Horizontal CSS padding, from the border box (`compute_bounds`) minus
/// the content box (`width()`).
fn chip_horizontal_padding(chip: &gtk4::Widget, origin: &gtk4::Widget) -> f32 {
    let bounds = chip.compute_bounds(origin).expect("chip bounds");
    bounds.width() - chip.width() as f32
}

fn assert_chip_cluster(window: &gtk4::Widget) {
    let header = try_header(window, "worker").expect("worker header");
    let title = label_with_class(&header, "pane-title");
    let subtitle = label_with_class(&header, "pane-subtitle");
    let chip = try_descendant(&header, "task-chip").expect("header chip");
    let actions = try_descendant(&header, "pane-actions").expect("pane actions");
    assert_eq!(title.text(), "worker");
    assert_eq!(subtitle.text(), "Idle");
    assert_eq!(
        visible_label_texts(&chip),
        vec![
            "Task".to_string(),
            "fix-parser".to_string(),
            "Working".to_string()
        ]
    );
    let title_box = widget_bounds(title.upcast_ref(), &header);
    let subtitle_box = widget_bounds(subtitle.upcast_ref(), &header);
    let chip_box = widget_bounds(&chip, &header);
    let actions_box = widget_bounds(&actions, &header);
    let gap = chip_box.x - (subtitle_box.x + subtitle_box.w);
    assert!(
        (4.0..20.0).contains(&gap),
        "chip detached from the title cluster by {gap}px"
    );
    assert!(
        chip_box.x >= title_box.x + title_box.w - 1.0,
        "chip overlaps the pane title"
    );
    assert!(
        chip_box.x + chip_box.w <= actions_box.x + 1.0,
        "chip overlaps the header actions"
    );
    assert!(
        !title.layout().is_ellipsized(),
        "pane title ellipsized while the chip is showing"
    );
    let slack = chip_content_slack(&chip);
    let padding = chip_horizontal_padding(&chip, &header);
    assert!(
        (6..=10).contains(&slack),
        "header chip content slack is {slack}px (width {}, padding {padding})",
        chip.width()
    );
    assert!(
        (14.0..18.0).contains(&padding),
        "header chip horizontal padding is {padding}px"
    );
    let tip = chip.tooltip_text().unwrap_or_default().to_string();
    assert!(tip.contains("fix-parser"), "{tip}");

    let tasks = try_descendant(window, "workspace-tasks").expect("sidebar chips");
    let side = try_descendant(&tasks, "task-chip").expect("sidebar chip");
    assert_eq!(
        visible_label_texts(&side),
        vec!["Task".to_string(), "Working".to_string()]
    );
    let side_slack = chip_content_slack(&side);
    let side_padding = chip_horizontal_padding(&side, &tasks);
    assert!(
        (2..=6).contains(&side_slack),
        "sidebar chip content slack is {side_slack}px (width {}, padding {side_padding})",
        side.width()
    );
    assert!(
        (14.0..18.0).contains(&side_padding),
        "sidebar chip horizontal padding is {side_padding}px"
    );
    let side_tip = side.tooltip_text().unwrap_or_default().to_string();
    assert!(side_tip.contains("fix-parser"), "{side_tip}");
}

fn find_matching_widget<T: glib::object::IsA<gtk4::Widget> + glib::types::StaticType>(
    root: &gtk4::Widget,
    predicate: &impl Fn(&T) -> bool,
) -> Option<T> {
    if let Ok(widget) = root.clone().downcast::<T>() {
        if predicate(&widget) {
            return Some(widget);
        }
    }
    let mut child = root.first_child();
    while let Some(w) = child {
        if let Some(found) = find_matching_widget(&w, predicate) {
            return Some(found);
        }
        child = w.next_sibling();
    }
    None
}

fn find_sidebar_row(app: &App, id: &str) -> Option<gtk4::ListBoxRow> {
    find_matching_widget::<gtk4::ListBoxRow>(app.sidebar.widget.upcast_ref(), &|row| {
        row.widget_name() == id
    })
}

fn find_disclosure_button(row: &gtk4::ListBoxRow) -> Option<gtk4::Button> {
    find_matching_widget::<gtk4::Button>(row.upcast_ref(), &|b| {
        b.has_css_class("workspace-disclosure")
    })
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn task_board_explains_truncated_done_history() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(360, 680);
    window.present();
    let settings = gtk4::Settings::default().unwrap();
    let old_font = settings.gtk_font_name();
    let style = adw::StyleManager::default();
    let old_scheme = style.color_scheme();
    let tasks = (0..25)
        .map(|i| {
            let mut value = chip_scene_task(&format!("Finished task {i:02}"));
            value["id"] = json!(format!("finished-{i}"));
            value["state"] = json!("completed");
            value["disposition"] = json!({"outcome": "merged"});
            value["updated_at"] =
                json!((chrono::Utc::now() - chrono::Duration::minutes(i)).to_rfc3339());
            serde_json::from_value::<signaltty_core::Task>(value).unwrap()
        })
        .collect::<Vec<_>>();
    for (count, scheme, name) in [
        (25, adw::ColorScheme::ForceLight, "light"),
        (25, adw::ColorScheme::ForceDark, "dark-large"),
        (20, adw::ColorScheme::ForceLight, "untruncated"),
    ] {
        style.set_color_scheme(scheme);
        if scheme == adw::ColorScheme::ForceDark {
            window.add_css_class("dark");
            settings.set_gtk_font_name(Some("Sans 18"));
        } else {
            window.remove_css_class("dark");
            settings.set_gtk_font_name(old_font.as_deref());
        }
        let board = crate::board::present(&window, &tasks[..count], |_, _| {});
        let dialog = &board.dialog;
        let root = dialog.child().unwrap();
        wait_ui(|| dialog.width() > 0);
        let done = try_descendant(&root, "board-column-done").unwrap();
        let scroll = find_matching_widget::<gtk4::ScrolledWindow>(&root, &|scroll| {
            scroll.hscrollbar_policy() == gtk4::PolicyType::Automatic
                && scroll.vscrollbar_policy() == gtk4::PolicyType::Never
        })
        .unwrap();
        scroll
            .hadjustment()
            .set_value(scroll.hadjustment().upper() - scroll.hadjustment().page_size());
        capture_workflow(&window, &format!("board-history-{name}"));
        assert!(has_label(&done, &format!("Done · {count}")));
        let list = find_widget::<gtk4::ListBox>(&done).unwrap();
        assert!(list.row_at_index(19).is_some() && list.row_at_index(20).is_none());
        assert!(has_label(
            list.row_at_index(0).unwrap().upcast_ref(),
            "Finished task 00"
        ));
        assert!(has_label(
            list.row_at_index(19).unwrap().upcast_ref(),
            "Finished task 19"
        ));
        assert_eq!(
            has_label(&done, "Showing latest 20 of 25"),
            count > 20,
            "truncated history needs explicit feedback"
        );
        if count > 20 {
            let notice = find_matching_widget::<gtk4::Label>(&done, &|label| {
                label.text() == "Showing latest 20 of 25"
            })
            .unwrap();
            let bounds = notice.compute_bounds(&done).unwrap();
            assert!(
                notice.is_mapped()
                    && bounds.x() >= 0.0
                    && bounds.x() + bounds.width() <= done.width() as f32
            );
        }
        dialog.force_close();
        wait_ui(|| window.visible_dialog().is_none());
    }
    settings.set_gtk_font_name(old_font.as_deref());
    style.set_color_scheme(old_scheme);
    window.destroy();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn open_task_board_preserves_identity_scroll_and_focus_on_updates() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.window.set_default_size(720, 600);
    app.window.present();
    let settle = || {
        let deadline = Instant::now() + Duration::from_millis(120);
        while Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let mut tasks = (0..35)
        .map(|i| {
            let mut value = chip_scene_task(&format!("Review task {i:02}"));
            value["id"] = json!(format!("review-{i:02}"));
            value["pane_id"] = json!(format!("pane-{i:02}"));
            value["state"] = json!("completed");
            value["updated_at"] =
                json!((chrono::Utc::now() - chrono::Duration::minutes(i)).to_rfc3339());
            value
        })
        .collect::<Vec<_>>();
    for task in &tasks {
        assert!(app
            .model
            .borrow_mut()
            .tasks
            .apply_event(signaltty_proto::event::TASK_CREATED, &json!({"task": task})));
    }
    app.present_board();
    let dialog = app.board_dialog.borrow().as_ref().unwrap().dialog.clone();
    wait_ui(|| dialog.width() > 0);
    settle();
    let root = dialog.child().unwrap();
    let list =
        find_matching_widget::<gtk4::ListBox>(&root, &|list| list.row_at_index(30).is_some())
            .unwrap();
    let row = list.row_at_index(20).unwrap();
    assert!(row.grab_focus());
    let vertical = list
        .ancestor(gtk4::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    wait_ui(|| vertical.vadjustment().value() > 0.0);
    // Let native focus scrolling finish before recording a stable viewport.
    settle();
    settle();
    settle();
    let horizontal = find_matching_widget::<gtk4::ScrolledWindow>(&root, &|scroll| {
        scroll.hscrollbar_policy() == gtk4::PolicyType::Automatic
            && scroll.vscrollbar_policy() == gtk4::PolicyType::Never
    })
    .unwrap();
    let x = horizontal.hadjustment().value();
    let y = vertical.vadjustment().value();
    tasks[20]["label"] = json!("Updated focused task");
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": tasks[20]}),
    );
    app.refresh_open_board();
    assert_eq!(
        app.board_dialog
            .borrow()
            .as_ref()
            .map(|board| &board.dialog),
        Some(&dialog),
        "task update replaced the board dialog"
    );
    wait_ui(|| has_label(row.upcast_ref(), "Updated focused task"));
    settle();
    assert_eq!(list.row_at_index(20).as_ref(), Some(&row));
    assert!(row.has_focus());
    assert!((vertical.vadjustment().value() - y).abs() <= 1.0);
    assert!((horizontal.hadjustment().value() - x).abs() <= 1.0);
    settle();
    capture_workflow(&app.window, "board-live-update");
    // A fresh navigation after the update must beat deferred focus restoration.
    app.refresh_open_board();
    let other = list.row_at_index(21).unwrap();
    assert!(other.grab_focus());
    horizontal.hadjustment().set_value(140.0);
    app.refresh_open_board();
    settle();
    assert!(
        (horizontal.hadjustment().value() - 140.0).abs() <= 1.0,
        "second update used stale navigation position"
    );
    assert!(
        other.has_focus(),
        "deferred refresh stole a newer focus choice"
    );
    app.refresh_open_board();
    horizontal.hadjustment().set_value(200.0);
    let user_y = vertical.vadjustment().value() + 20.0;
    vertical.vadjustment().set_value(user_y);
    settle();
    assert!(
        (horizontal.hadjustment().value() - 200.0).abs() <= 1.0,
        "pending update overwrote manual horizontal scrolling"
    );
    assert!(
        (vertical.vadjustment().value() - user_y).abs() <= 1.0,
        "pending update overwrote manual vertical scrolling"
    );
    assert!(row.grab_focus());
    settle();
    let anchor = (0..35)
        .filter_map(|i| list.row_at_index(i))
        .find(|row| {
            row.compute_bounds(&list)
                .is_some_and(|b| f64::from(b.y() + b.height()) > vertical.vadjustment().value())
        })
        .unwrap();
    let offset =
        f64::from(anchor.compute_bounds(&list).unwrap().y()) - vertical.vadjustment().value();
    dialog.set_focus(None::<&gtk4::Widget>);
    let mut inserted = tasks[0].clone();
    inserted["id"] = json!("newest-review");
    inserted["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_CREATED,
        &json!({"task": inserted}),
    );
    app.refresh_open_board();
    settle();
    assert!(
        (f64::from(anchor.compute_bounds(&list).unwrap().y())
            - vertical.vadjustment().value()
            - offset)
            .abs()
            <= 1.0,
        "insertion above viewport moved its anchor"
    );
    let x = horizontal.hadjustment().value();
    tasks[0]["disposition"] = json!({"outcome": "merged"});
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": tasks[0]}),
    );
    app.refresh_open_board();
    settle();
    assert!(
        (horizontal.hadjustment().value() - x).abs() <= 1.0,
        "unfocused movement changed horizontal position"
    );
    app.window.set_default_size(360, 600);
    settle();
    assert!(row.grab_focus());
    tasks[20]["pr"] = json!({"number": 12, "url": "https://github.com/example/repo/pull/12", "state": "open", "checks": "failing"});
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": tasks[20]}),
    );
    app.refresh_open_board();
    settle();
    assert!(row.has_focus(), "focus must follow the task to Needs you");
    tasks[20]["pr"]["checks"] = json!("passing");
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_UPDATED,
        &json!({"task": tasks[20]}),
    );
    app.refresh_open_board();
    app.refresh_open_board();
    settle();
    assert!(row.has_focus(), "a burst of updates lost the focused task");
    wait_ui(|| {
        row.compute_bounds(&horizontal).is_some_and(|bounds| {
            bounds.x() >= -1.0 && bounds.x() + bounds.width() <= horizontal.width() as f32 + 1.0
        })
    });
    let bounds = row.compute_bounds(&horizontal).unwrap();
    assert!(
        bounds.x() >= -1.0 && bounds.x() + bounds.width() <= horizontal.width() as f32 + 1.0,
        "focused task x={} width={} viewport={} adjustment={}",
        bounds.x(),
        bounds.width(),
        horizontal.width(),
        horizontal.hadjustment().value()
    );
    let pill = find_matching_widget::<gtk4::Label>(row.upcast_ref(), &|label| {
        label.has_css_class("task-chip")
    })
    .unwrap();
    assert!(pill.has_css_class("task-completed"));
    tasks.retain(|task| task["id"] != "review-20");
    {
        let mut model = app.model.borrow_mut();
        let ticket = model.tasks.seed_ticket();
        model.tasks.complete_seed(
            ticket,
            tasks
                .iter()
                .cloned()
                .map(|task| serde_json::from_value(task).unwrap())
                .collect(),
        );
    }
    app.refresh_open_board();
    settle();
    assert!(row.parent().is_none());
    assert_eq!(
        app.board_dialog
            .borrow()
            .as_ref()
            .map(|board| &board.dialog),
        Some(&dialog)
    );
    assert!(
        dialog.focus().is_some(),
        "removed focused card leaves native dialog focus"
    );
    {
        let mut model = app.model.borrow_mut();
        let ticket = model.tasks.seed_ticket();
        model.tasks.complete_seed(ticket, Vec::new());
    }
    app.refresh_open_board();
    settle();
    assert!(has_label(&dialog.child().unwrap(), "No Tasks Yet"));
    app.model.borrow_mut().tasks.apply_event(
        signaltty_proto::event::TASK_CREATED,
        &json!({"task": tasks[1]}),
    );
    app.refresh_open_board();
    settle();
    assert_eq!(
        app.board_dialog
            .borrow()
            .as_ref()
            .map(|board| &board.dialog),
        Some(&dialog)
    );
    assert!(has_label(
        &dialog.child().unwrap(),
        tasks[1]["label"].as_str().unwrap()
    ));
    for scheme in [adw::ColorScheme::ForceLight, adw::ColorScheme::ForceDark] {
        adw::StyleManager::default().set_color_scheme(scheme);
        settle();
        capture_workflow(
            &app.window,
            if scheme == adw::ColorScheme::ForceLight {
                "board-live-light"
            } else {
                "board-live-dark"
            },
        );
    }

    app.refresh_open_board();
    dialog.force_close();
    wait_ui(|| app.board_dialog.borrow().is_none());
    settle();
    assert!(app.board_dialog.borrow().is_none());
    app.window.destroy();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn task_board_live_activation_and_neighbor_fallback_use_current_rows() {
    adw::init().unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(1200, 680);
    window.present();
    let settle = || {
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            while glib::MainContext::default().iteration(false) {}
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let mut tasks = (0..35)
        .map(|i| {
            let mut value = chip_scene_task(&format!("Task {i:02}"));
            value["id"] = json!(format!("task-{i:02}"));
            value["state"] = json!("completed");
            value["updated_at"] =
                json!((chrono::Utc::now() - chrono::Duration::minutes(i)).to_rfc3339());
            serde_json::from_value::<signaltty_core::Task>(value).unwrap()
        })
        .collect::<Vec<_>>();
    let chosen = Rc::new(RefCell::new(None));
    let target = chosen.clone();
    let board = crate::board::present(&window, &tasks, move |_, pane| {
        *target.borrow_mut() = pane;
    });
    wait_ui(|| board.dialog.width() > 0);
    settle();
    let root = board.dialog.child().unwrap();
    let list =
        find_matching_widget::<gtk4::ListBox>(&root, &|list| list.row_at_index(30).is_some())
            .unwrap();
    let focused = list.row_at_index(20).unwrap();
    let next = list.row_at_index(21).unwrap();
    let previous = list.row_at_index(19).unwrap();
    assert!(focused.grab_focus());
    settle();
    settle();
    // Removing the focused row while its next neighbor changes columns must
    // choose the previous row in the original column.
    tasks[21].pr = serde_json::from_value(json!({"number": 12, "url": "https://github.com/example/repo/pull/12", "state": "open", "checks": "failing"})).ok();
    assert!(tasks[21].pr.is_some());
    tasks.remove(20);
    board.update(&tasks);
    board.update(&tasks);
    settle();
    assert!(previous.has_focus());
    assert_ne!(previous.parent(), next.parent());
    assert!(focused.parent().is_none());
    settle();
    let vertical = list
        .ancestor(gtk4::ScrolledWindow::static_type())
        .and_downcast::<gtk4::ScrolledWindow>()
        .unwrap();
    let anchor = (0..35)
        .filter_map(|i| list.row_at_index(i))
        .find(|row| {
            row.compute_bounds(&list)
                .is_some_and(|b| f64::from(b.y() + b.height()) > vertical.vadjustment().value())
        })
        .unwrap();
    let offset =
        f64::from(anchor.compute_bounds(&list).unwrap().y()) - vertical.vadjustment().value();
    let mut inserted = tasks[0].clone();
    inserted.id = "newest".into();
    inserted.updated_at = chrono::Utc::now();
    tasks.push(inserted);
    board.update(&tasks);
    // A new choice in another column owns focus, but does not abandon this anchor.
    assert!(next.grab_focus());
    board.update(&tasks);
    settle();
    settle();
    assert!(next.has_focus());
    assert!(
        (f64::from(anchor.compute_bounds(&list).unwrap().y())
            - vertical.vadjustment().value()
            - offset)
            .abs()
            <= 1.0
    );
    // Shrinking a nearly-bottom viewport makes GTK clamp its adjustment.
    // That automatic change must not be mistaken for new user scrolling.
    board.dialog.set_focus(None::<&gtk4::Widget>);
    vertical
        .vadjustment()
        .set_value(vertical.vadjustment().upper() - vertical.vadjustment().page_size() - 32.0);
    settle();
    let anchor = (0..35)
        .filter_map(|i| list.row_at_index(i))
        .find(|row| {
            row.compute_bounds(&list)
                .is_some_and(|b| f64::from(b.y() + b.height()) > vertical.vadjustment().value())
        })
        .unwrap();
    let offset =
        f64::from(anchor.compute_bounds(&list).unwrap().y()) - vertical.vadjustment().value();
    let removed = (0..10)
        .map(|i| list.row_at_index(i).unwrap().widget_name())
        .collect::<Vec<_>>();
    tasks.retain(|task| !removed.iter().any(|id| id.as_str() == task.id));
    board.update(&tasks);
    board.update(&tasks);
    settle();
    settle();
    assert!(
        (f64::from(anchor.compute_bounds(&list).unwrap().y())
            - vertical.vadjustment().value()
            - offset)
            .abs()
            <= 1.0,
        "GTK clamp after removals lost the surviving viewport anchor: y={} value={} old_offset={}",
        anchor.compute_bounds(&list).unwrap().y(),
        vertical.vadjustment().value(),
        offset
    );
    let id = previous.widget_name();
    let task = tasks
        .iter_mut()
        .find(|task| task.id == id.as_str())
        .unwrap();
    task.pane_id = None;
    board.update(&tasks);
    settle();
    assert!(!previous.is_activatable());
    let task = tasks
        .iter_mut()
        .find(|task| task.id == id.as_str())
        .unwrap();
    task.pane_id = Some("new-current-pane".into());
    board.update(&tasks);
    settle();
    assert!(previous.is_activatable());
    assert!(previous.activate());
    wait_ui(|| chosen.borrow().is_some());
    assert_eq!(chosen.borrow().as_deref(), Some("new-current-pane"));
    wait_ui(|| window.visible_dialog().is_none());
    window.destroy();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session (or xvfb-run)"]
fn task_workspace_renders_as_child_and_collapses() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let state = Arc::new(Mutex::new(BTreeMap::from([
        ("root".to_string(), fixture("root")),
        ("child".to_string(), fixture("child")),
    ])));
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call {
                method,
                params,
                reply,
                ..
            } = request
            {
                if method == "test.stop" {
                    let _ = reply.send(Ok(Value::Null));
                    break;
                }
                let st = state.lock().unwrap();
                let result = match method.as_str() {
                    "workspace.list" => Ok(json!({
                        "workspaces": st.values().map(|s| s["workspace"].clone()).collect::<Vec<_>>()
                    })),
                    "workspace.get" => st
                        .get(params["workspace_id"].as_str().unwrap())
                        .cloned()
                        .ok_or("NO_SUCH_WORKSPACE".into()),
                    "task.list" => {
                        let now = chrono::Utc::now();
                        let task = signaltty_core::Task {
                            id: "task_1".into(),
                            context_id: "tctx_1".into(),
                            parent_task_id: None,
                            pane_id: Some("pane_child".into()),
                            parent_pane_id: Some("pane_root".into()),
                            root_pane_id: Some("pane_root".into()),
                            relationship: signaltty_core::Relationship::Subagent,
                            label: "Subtask".into(),
                            contract: signaltty_core::Contract::new("Do work").unwrap(),
                            agent: None,
                            source_repo: std::path::PathBuf::from("/tmp"),
                            target_branch: None,
                            worktree_path: std::path::PathBuf::from("/tmp"),
                            branch: "task/subtask".into(),
                            preexisting_branch: false,
                            base_ref: "HEAD".into(),
                            base_sha: "123456".into(),
                            state: signaltty_core::TaskState::Working,
                            result: None,
                            disposition: signaltty_core::Disposition::default(),
                            pr: None,
                            status_reason: None,
                            finish_error: None,
                            worker_pid: None,
                            worker_cmd: None,
                            client_request_id: None,
                            created_at: now,
                            updated_at: now,
                        };
                        Ok(json!({
                            "tasks": [serde_json::to_value(&task).unwrap()]
                        }))
                    }
                    _ => panic!("unexpected IPC {method}"),
                };
                let _ = reply.send(result);
            }
        }
    });
    let (ui, _events) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    drain_refresh(&app);

    // Root row and child row should both exist in order root -> child
    let root_row = find_sidebar_row(&app, "root").expect("root row exists");
    let child_row = find_sidebar_row(&app, "child").expect("child row exists");

    assert!(
        !root_row.has_css_class("workspace-child"),
        "root row is not a child"
    );
    assert!(
        child_row.has_css_class("workspace-child"),
        "child row has workspace-child class"
    );
    assert!(
        !child_row.has_css_class("workspace-finished"),
        "active child row does not have workspace-finished class"
    );
    assert!(child_row.get_visible(), "child row is visible initially");

    // Parent row shows disclosure button with "▾ 1 task"
    let disclosure = find_disclosure_button(&root_row).expect("disclosure button on root");
    assert!(disclosure.get_visible(), "disclosure button is visible");
    assert!(disclosure
        .label()
        .as_deref()
        .unwrap_or("")
        .contains("1 task"));
    assert!(disclosure.label().as_deref().unwrap_or("").starts_with("▾"));

    // Clicking disclosure collapses the group
    disclosure.emit_clicked();
    assert!(
        !child_row.get_visible(),
        "child row is hidden when collapsed"
    );
    assert!(disclosure.label().as_deref().unwrap_or("").starts_with("▸"));

    // Clicking disclosure again expands the group
    disclosure.emit_clicked();
    assert!(
        child_row.get_visible(),
        "child row is visible when expanded"
    );
    assert!(disclosure.label().as_deref().unwrap_or("").starts_with("▾"));

    // Collapse again, then select child workspace -> auto-expands
    disclosure.emit_clicked();
    assert!(!child_row.get_visible());
    app.sidebar.select("child");
    assert!(
        child_row.get_visible(),
        "selecting child auto-expands the group"
    );
    assert!(disclosure.label().as_deref().unwrap_or("").starts_with("▾"));
    assert!(child_row.is_selected(), "child row is selected");

    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    drop(app);
    worker.join().unwrap();
}

#[test]
#[ignore = "requires a GTK display; run with dbus-run-session (or xvfb-run)"]
fn finished_task_workspace_has_workspace_finished_css_class() {
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    let (actor, mut requests) = IpcHandle::test_channel();
    let state = Arc::new(Mutex::new(BTreeMap::from([
        ("root".to_string(), fixture("root")),
        ("child".to_string(), fixture("child")),
    ])));
    let worker = std::thread::spawn(move || {
        while let Some(request) = requests.blocking_recv() {
            if let ActorRequest::Call {
                method,
                params,
                reply,
                ..
            } = request
            {
                if method == "test.stop" {
                    let _ = reply.send(Ok(Value::Null));
                    break;
                }
                let st = state.lock().unwrap();
                let result = match method.as_str() {
                    "workspace.list" => Ok(json!({
                        "workspaces": st.values().map(|s| s["workspace"].clone()).collect::<Vec<_>>()
                    })),
                    "workspace.get" => st
                        .get(params["workspace_id"].as_str().unwrap())
                        .cloned()
                        .ok_or("NO_SUCH_WORKSPACE".into()),
                    "task.list" => {
                        let now = chrono::Utc::now();
                        let task = signaltty_core::Task {
                            id: "task_1".into(),
                            context_id: "tctx_1".into(),
                            parent_task_id: None,
                            pane_id: Some("pane_child".into()),
                            parent_pane_id: Some("pane_root".into()),
                            root_pane_id: Some("pane_root".into()),
                            relationship: signaltty_core::Relationship::Subagent,
                            label: "Subtask".into(),
                            contract: signaltty_core::Contract::new("Do work").unwrap(),
                            agent: None,
                            source_repo: std::path::PathBuf::from("/tmp"),
                            target_branch: None,
                            worktree_path: std::path::PathBuf::from("/tmp"),
                            branch: "task/subtask".into(),
                            preexisting_branch: false,
                            base_ref: "HEAD".into(),
                            base_sha: "123456".into(),
                            state: signaltty_core::TaskState::Completed,
                            result: None,
                            disposition: signaltty_core::Disposition {
                                outcome: signaltty_core::DispositionOutcome::Merged,
                                ..Default::default()
                            },
                            pr: None,
                            status_reason: None,
                            finish_error: None,
                            worker_pid: None,
                            worker_cmd: None,
                            client_request_id: None,
                            created_at: now,
                            updated_at: now,
                        };
                        Ok(json!({
                            "tasks": [serde_json::to_value(&task).unwrap()]
                        }))
                    }
                    _ => panic!("unexpected IPC {method}"),
                };
                let _ = reply.send(result);
            }
        }
    });
    let (ui, _events) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    drain_refresh(&app);

    let root_row = find_sidebar_row(&app, "root").expect("root row exists");
    let child_row = find_sidebar_row(&app, "child").expect("child row exists");

    assert!(child_row.has_css_class("workspace-child"));
    assert!(
        child_row.has_css_class("workspace-finished"),
        "finished child row has workspace-finished css class"
    );

    let disclosure = find_disclosure_button(&root_row).expect("disclosure button on root");
    assert!(disclosure.get_visible(), "disclosure button is visible");
    assert!(disclosure
        .label()
        .as_deref()
        .unwrap_or("")
        .contains("1 done"));

    glib::MainContext::default()
        .block_on(app.actor.call("test.stop", json!({})))
        .unwrap();
    app.window.destroy();
    drop(app);
    worker.join().unwrap();
}

fn widgets_with_class(root: &gtk4::Widget, class: &str, out: &mut Vec<gtk4::Widget>) {
    if root.has_css_class(class) {
        out.push(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        widgets_with_class(&widget, class, out);
        child = widget.next_sibling();
    }
}

/// Preferences show each scheme as a miniature window and each theme as
/// a light and a dark orb; choosing one applies and persists it.
#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn preferences_preview_schemes_and_themes() {
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    let config = std::env::temp_dir().join(format!("signaltty-prefs-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let (actor, _requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    app.window.set_default_size(920, 680);
    app.window.present();
    let dialog = crate::preferences::build_dialog(&app);
    dialog.present(Some(&app.window));
    let root = dialog.clone().upcast::<gtk4::Widget>();
    wait_ui(|| dialog.is_mapped());

    let mut previews = Vec::new();
    widgets_with_class(&root, "scheme-preview", &mut previews);
    assert_eq!(
        previews.len(),
        3,
        "System, Light and Dark each get a preview"
    );
    for preview in &previews {
        wait_ui(|| preview.width() > 0);
        assert!(preview.width() >= 148 && preview.height() >= 92);
    }
    // System splits its content: light on the left, dark on the right.
    let mut halves = Vec::new();
    widgets_with_class(&previews[0], "mw-content", &mut halves);
    assert!(halves[0].has_css_class("light") && halves[1].has_css_class("dark"));
    // ...split where its calc(21px + 50%) background does, so the
    // backing never shows as a step or hairline at the seam.
    let split = halves[1].compute_bounds(&previews[0]).unwrap().x();
    let expected = 21.0 + previews[0].width() as f32 / 2.0;
    assert!(
        (split - expected).abs() <= 1.0,
        "System split at {split}, background at {expected}"
    );
    // Every part stays left to right, as its CSS is drawn, in RTL too.
    for preview in &previews {
        let mut stack = vec![preview.clone()];
        while let Some(widget) = stack.pop() {
            assert_eq!(widget.direction(), gtk4::TextDirection::Ltr);
            let mut child = widget.first_child();
            while let Some(next) = child {
                child = next.next_sibling();
                stack.push(next);
            }
        }
    }
    let mut orbs = Vec::new();
    widgets_with_class(&root, "theme-swatch", &mut orbs);
    assert_eq!(orbs.len(), 2 * signaltty_core::theme::Theme::ALL.len());

    for (name, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        while glib::MainContext::default().iteration(false) {}
        capture_workflow(&app.window, &format!("preferences-{name}"));
    }
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    let grove = find_matching_widget::<gtk4::Button>(&root, &|button| {
        button.has_css_class("theme-card") && has_label(button.upcast_ref(), "Grove")
    })
    .unwrap();
    grove.emit_clicked();
    assert_eq!(app.preference().theme, signaltty_core::theme::Theme::Grove);
    assert!(grove.has_css_class("selected"));
    let mut selected = Vec::new();
    widgets_with_class(&root, "selected", &mut selected);
    assert_eq!(selected.len(), 2, "one scheme and one theme stay selected");
    assert_eq!(
        crate::preferences::load_preference().theme,
        signaltty_core::theme::Theme::Grove,
        "the choice persists"
    );

    dialog.close();
    app.window.destroy();
    let _ = std::fs::remove_dir_all(&config);
    match previous_config {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
}

/// Answer unrelated calls with null until `method` arrives.
fn call_named(
    requests: &mut tokio::sync::mpsc::UnboundedReceiver<ActorRequest>,
    method: &str,
) -> (Value, tokio::sync::oneshot::Sender<Result<Value, String>>) {
    loop {
        wait_ui(|| !requests.is_empty());
        if let ActorRequest::Call {
            method: name,
            params,
            reply,
            ..
        } = requests.try_recv().unwrap()
        {
            if name == method {
                return (params, reply);
            }
            let _ = reply.send(Ok(Value::Null));
        }
    }
}

/// The breadcrumb opens a details card: branch, path, task and PR, and
/// change totals read on open; Changes opens the docked panel.
#[test]
#[ignore = "requires a GTK display; run with dbus-run-session"]
fn breadcrumb_opens_the_workspace_details_card() {
    let previous_config = std::env::var_os("XDG_CONFIG_HOME");
    let config = std::env::temp_dir().join(format!("signaltty-details-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &config);
    std::env::set_var("SIGNALTTY_NOTIFY", "0");
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.register(None::<&gio::Cancellable>).unwrap();
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    let display = gtk4::gdk::Display::default().unwrap();
    let provider = gtk4::CssProvider::new();
    provider.load_from_resource("/dev/signaltty/gui/style.css");
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk4::IconTheme::for_display(&display).add_resource_path("/dev/signaltty/gui/icons");
    let (actor, mut requests) = IpcHandle::test_channel();
    let (ui, _) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    app.window.set_default_size(1280, 760);
    app.window.present();
    assert!(!app.crumb_button.is_sensitive(), "no workspace, no details");

    let snapshot = chip_scene_snapshot();
    let mut task = chip_scene_task("fix-parser");
    task["pr"] = json!({
        "number": 77,
        "url": "https://github.com/example/repo/pull/77",
        "state": "open",
        "checks": "passing"
    });
    {
        let mut model = app.model.borrow_mut();
        model.cache.workspaces = vec![snapshot.workspace.clone()];
        model
            .cache
            .snapshots
            .insert(snapshot.workspace.id.clone(), snapshot.clone());
        assert!(model
            .tasks
            .apply_event(signaltty_proto::event::TASK_CREATED, &json!({"task": task})));
    }
    app.show_workspace("fix");
    assert!(app.crumb_button.is_sensitive());

    app.crumb_button.popup();
    let card = app.details.popover.clone().upcast::<gtk4::Widget>();
    let (params, reply) = call_named(&mut requests, "workspace.diff");
    assert_eq!(params["workspace_id"], "fix");
    reply
        .send(Ok(json!({"added": 12, "removed": 3, "files": [
            {"path": "src/parser.rs", "added": 12, "removed": 3, "untracked": false, "binary": false}
        ]})))
        .unwrap();
    wait_ui(|| has_label(&card, "+12") && has_label(&card, "−3"));
    assert!(has_label(&card, "fix/parser"));
    assert!(has_label(
        &card,
        &crate::util::tilde(&snapshot.workspace.cwd)
    ));
    assert!(has_label(&card, "Task fix-parser · Working"));
    assert!(has_label(&card, "#77") && has_label(&card, "Checks passing"));
    for (name, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        while glib::MainContext::default().iteration(false) {}
        capture_workflow(&app.window, &format!("details-{name}"));
    }
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    // Copy Path puts the full path on the clipboard.
    button_with_tooltip(&card, "Copy Path")
        .unwrap()
        .emit_clicked();
    let copied = Rc::new(RefCell::new(None));
    let capture = copied.clone();
    card.clipboard()
        .read_text_async(None::<&gio::Cancellable>, move |result| {
            *capture.borrow_mut() = Some(result.unwrap().unwrap().to_string())
        });
    wait_ui(|| copied.borrow().is_some());
    assert_eq!(
        copied.borrow().as_deref(),
        Some(snapshot.workspace.cwd.as_str())
    );

    // Changes hands over to the docked panel and closes the card.
    button_with_tooltip(&card, "Show Changes (Ctrl+Shift+D)")
        .unwrap()
        .emit_clicked();
    wait_ui(|| !app.details.popover.is_visible());
    assert!(app.changes_split.shows_sidebar());

    card.clipboard().set_text("");
    app.window.destroy();
    let _ = std::fs::remove_dir_all(&config);
    match previous_config {
        Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
}
