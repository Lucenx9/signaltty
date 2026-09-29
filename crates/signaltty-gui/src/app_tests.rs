use super::*;
use crate::actor::{ActorRequest, SnapshotReply};
use crate::refresh::tests::fixture;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn emit(app: &App, name: &str, payload: Value) {
    app.on_event(UiEvent::ServerEvent {
        name: name.into(),
        payload,
    });
}

fn drain_refresh(app: &App) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.refresh_scheduled.get() {
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
                ActorRequest::Attach { reply, .. } => {
                    let _ = reply.send(Ok(SnapshotReply {
                        snapshot: Vec::new(),
                    }));
                }
                ActorRequest::Detach { .. } => {}
            }
        }
    });
    let (ui, _events) = tokio::sync::mpsc::unbounded_channel();
    let app = App::new(&application, actor, ui);
    app.refresh();
    assert_eq!(
        calls.lock().unwrap().len(),
        3,
        "one list + one get per workspace"
    );
    assert_eq!(app.active_ws_id().as_deref(), Some("a"));
    assert_sidebar(&app, &["a", "b"], "a");
    let original_page = app.tab_view.selected_page().unwrap();
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
    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            ("pane.get".into(), json!({"pane_id": "pane_b"})),
            ("pane.get".into(), json!({"pane_id": "pane_b"})),
        ]
    );
    drain_refresh(&app);
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
    assert!(app.widgets.borrow().is_empty());
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

    app.actor.call("test.stop", json!({})).unwrap();
    app.window.destroy();
    drop(app);
    worker.join().unwrap();
}
