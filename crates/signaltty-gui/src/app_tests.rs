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
        gtk4::prelude::WidgetExt::activate_action(&app.window, "win.show-changes", None).unwrap();
        let dialog = app.window.visible_dialog().unwrap();
        wait_ui(|| {
            has_label(
                &dialog.child().unwrap(),
                "Changes against HEAD · 3 files · +12 −3",
            )
        });
        assert!(has_label(&dialog.child().unwrap(), "src/<changed>&.rs"));
        assert!(has_label(&dialog.child().unwrap(), "Binary"));
        assert!(has_label(&dialog.child().unwrap(), "Untracked"));
        capture_workflow(&app.window, &format!("changes-{theme}"));
        dialog.close();
        wait_ui(|| app.window.visible_dialog().is_none());
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
    let narrow = app.window.visible_dialog().unwrap();
    wait_ui(|| has_label(&narrow.child().unwrap(), "Binary"));
    capture_workflow(&app.window, "changes-narrow");
    narrow.close();
    wait_ui(|| app.window.visible_dialog().is_none());
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
fn file_diff_reader_uses_native_numbered_selectable_controls() {
    adw::init().unwrap();
    gio::resources_register_include!("signaltty-gui.gresource").unwrap();
    let application = adw::Application::new(None, gio::ApplicationFlags::NON_UNIQUE);
    application.set_resource_base_path(Some("/dev/signaltty/gui"));
    application.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&application);
    window.set_default_size(920, 680);
    let terminal = gtk4::Label::new(Some("Workspace terminals stay mounted"));
    window.set_content(Some(&terminal));
    window.present();
    let (actor, mut requests) = IpcHandle::test_channel();
    crate::workspace_dialogs::changes(&window, actor.clone(), "a");
    let dialog = window.visible_dialog().unwrap();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    let summary = json!({"added":2,"removed":1,"files":[
        {"path":"src/<changed>&.rs","added":2,"removed":1,"binary":false,"untracked":false},
        {"path":"notes/new <file>&.txt","added":0,"removed":0,"binary":false,"untracked":true},
        {"path":"assets/banner.png","added":0,"removed":0,"binary":true,"untracked":false}
    ]});
    reply.send(Ok(summary.clone())).unwrap();
    wait_ui(|| has_label(&dialog.child().unwrap(), "src/<changed>&.rs"));
    assert!(
        requests.is_empty(),
        "opening summary must not automatically read a file"
    );
    let row = action_row_with_title(&dialog.child().unwrap(), "src/<changed>&.rs").unwrap();
    let rows = find_widget::<gtk4::ListBox>(&dialog.child().unwrap()).unwrap();
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
    let reader = find_widget::<gtk4::TextView>(&dialog.child().unwrap()).unwrap();
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
    assert!(has_label(&dialog.child().unwrap(), "Changes against HEAD"));
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
    let navigation = find_widget::<adw::NavigationView>(&dialog.child().unwrap()).unwrap();
    let back =
        button_with_label(&navigation.visible_page().unwrap().child().unwrap(), "Back").unwrap();
    back.emit_clicked();
    assert_eq!(
        rows.selected_row().unwrap(),
        row.clone().upcast::<gtk4::ListBoxRow>()
    );
    assert_eq!(
        dialog.focus().unwrap(),
        row.clone().upcast::<gtk4::Widget>()
    );
    assert_eq!(list_scroll.vadjustment().value(), scroll);
    assert!(requests.is_empty());
    // The list and actual row survive refresh; only explicit activation reads.
    let list_refresh = button_with_label(
        &navigation.visible_page().unwrap().child().unwrap(),
        "Refresh",
    )
    .unwrap();
    list_refresh.emit_clicked();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply.send(Ok(summary.clone())).unwrap();
    wait_ui(|| {
        has_label(
            &dialog.child().unwrap(),
            "Changes against HEAD · 3 files · +2 −1",
        )
    });
    assert_eq!(
        action_row_with_title(&dialog.child().unwrap(), "src/<changed>&.rs").unwrap(),
        row
    );
    assert!(requests.is_empty());
    // A pending read cannot overwrite B after Back and a new activation.
    row.emit_by_name::<()>("activated", &[]);
    let (_, old_reply) = next_diff_call(&mut requests, "workspace.file_diff");
    back.emit_clicked();
    let new_row = action_row_with_title(&dialog.child().unwrap(), "notes/new <file>&.txt").unwrap();
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
    wait_ui(|| {
        has_label(
            &dialog.child().unwrap(),
            "Untracked · Preview truncated at 10,000 lines.",
        )
    });
    old_reply
        .send(Ok(
            json!({"path":"src/<changed>&.rs","untracked":false,"content":{"kind":"binary"}}),
        ))
        .unwrap();
    while glib::MainContext::default().iteration(false) {}
    assert!(reader_text().contains("New content <>&"));
    assert!(!has_label(
        &dialog.child().unwrap(),
        "Binary file changed. No text preview is available."
    ));
    capture_workflow(&window, "file-diff-incomplete");
    // Refresh invalidates the displayed patch and its pending response.
    new_row.emit_by_name::<()>("activated", &[]);
    let (_, stale_refresh) = next_diff_call(&mut requests, "workspace.file_diff");
    let refresh = button_with_label(
        &navigation.visible_page().unwrap().child().unwrap(),
        "Refresh",
    )
    .unwrap();
    refresh.emit_clicked();
    assert!(reader_text().is_empty());
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply.send(Ok(summary.clone())).unwrap();
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply.send(Ok(json!({"path":"notes/new <file>&.txt","untracked":true,"content":{"kind":"text","hunks":[],"truncated":false,"notice":"Empty untracked file."}}))).unwrap();
    wait_ui(|| {
        has_label(
            &dialog.child().unwrap(),
            "Untracked · Empty untracked file.",
        )
    });
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
            &dialog.child().unwrap(),
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
    let binary = action_row_with_title(&dialog.child().unwrap(), "assets/banner.png").unwrap();
    binary.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply
        .send(Ok(
            json!({"path":"assets/banner.png","untracked":false,"content":{"kind":"binary"}}),
        ))
        .unwrap();
    wait_ui(|| {
        has_label(
            &dialog.child().unwrap(),
            "Binary file changed. No text preview is available.",
        )
    });
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
        wait_ui(|| has_label(&dialog.child().unwrap(), notice));
        assert!(reader_text().is_empty());
    }

    row.emit_by_name::<()>("activated", &[]);
    let (_, reply) = next_diff_call(&mut requests, "workspace.file_diff");
    reply
        .send(Ok(numbered_diff_fixture("src/<changed>&.rs")))
        .unwrap();
    wait_ui(|| reader_text().contains("after"));
    window.set_default_size(360, 680);
    wait_ui(|| window.width() <= 400);
    wait_ui(|| reader_scroll.width() <= 360);
    assert!(back.width() > 0 && back.width() < 100);
    capture_workflow(&window, "file-diff-narrow");
    assert_eq!(window.content().unwrap(), terminal.upcast::<gtk4::Widget>());
    // Closing an in-flight read neither resurrects it nor affects a new dialog.
    row.emit_by_name::<()>("activated", &[]);
    let (_, stale_closed) = next_diff_call(&mut requests, "workspace.file_diff");
    let weak_reader = reader.downgrade();
    let closed = Rc::new(Cell::new(false));
    let closing = closed.clone();
    dialog.connect_closed(move |_| closing.set(true));
    dialog.close();
    wait_ui(|| window.visible_dialog().is_none() && closed.get());
    crate::workspace_dialogs::changes(&window, actor, "a");
    let replacement = window.visible_dialog().unwrap();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply
        .send(Ok(json!({"added":0,"removed":0,"files":[]})))
        .unwrap();
    stale_closed
        .send(Ok(numbered_diff_fixture("src/<changed>&.rs")))
        .unwrap();
    wait_ui(|| has_label(&replacement.child().unwrap(), "No working tree changes"));
    assert!(!has_label(
        &replacement.child().unwrap(),
        "src/<changed>&.rs"
    ));
    reader.clipboard().set_text("");
    reader.primary_clipboard().set_text("");
    drop(buffer);
    drop(reader_scroll);
    drop(list_scroll);
    drop(navigation);
    drop(back);
    drop(list_refresh);
    drop(refresh);
    drop(row);
    drop(rows);
    drop(new_row);
    drop(binary);
    drop(reader);
    drop(dialog);
    wait_ui(|| weak_reader.upgrade().is_none());
    button_with_label(&replacement.child().unwrap(), "Refresh")
        .unwrap()
        .emit_clicked();
    let (_, reply) = next_diff_call(&mut requests, "workspace.diff");
    reply
        .send(Ok(json!({"added":0,"removed":0,"files":[
            {"path":"-option","added":0,"removed":0,"binary":false,"untracked":true}
        ]})))
        .unwrap();
    wait_ui(|| has_label(&replacement.child().unwrap(), "-option"));
    assert!(!has_label(
        &replacement.child().unwrap(),
        "No working tree changes"
    ));
    replacement.close();
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
