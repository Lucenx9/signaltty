use super::*;
use crate::actor::ActorRequest;
use crate::refresh::tests::fixture;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
        3,
        "one list + one get per workspace"
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
    assert_eq!(calls.lock().unwrap().len(), 4);
    assert!(has_label(app.sidebar.widget.upcast_ref(), "c"));
    assert!(!app.banner.is_revealed());
    assert_sidebar(&app, &["a", "c", "b"], "b");
    calls.lock().unwrap().clear();

    state.lock().unwrap().remove("b");
    emit(&app, "workspace.closed", json!({"workspace_id": "b"}));
    drain_refresh(&app);
    assert_eq!(calls.lock().unwrap().len(), 3);
    assert_eq!(app.active_ws_id().as_deref(), Some("a"));
    assert_eq!(app.title.title(), "a");
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
