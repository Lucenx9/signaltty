//! Spec 041: `layout.export` describes a tab; `layout.apply` starts one.

use serde_json::{json, Value};
use signaltty_testkit::TestServer;

fn leaves(layout: &Value) -> Vec<Value> {
    match layout["type"].as_str() {
        Some("pane") => vec![layout.clone()],
        Some("split") => [leaves(&layout["first"]), leaves(&layout["second"])].concat(),
        _ => vec![],
    }
}

#[tokio::test]
async fn an_exported_layout_applies_as_a_new_tab() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let created = c.call("workspace.create", json!({})).await.unwrap();
    let ws = created["workspace"]["id"].as_str().unwrap().to_string();
    let cwd = created["workspace"]["cwd"].as_str().unwrap().to_string();
    let a = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b = c
        .call(
            "pane.split",
            json!({"pane_id": a, "direction": "down", "argv": ["sh", "-c", "echo hi; sleep 30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.call("pane.rename", json!({"pane_id": b, "name": "worker"}))
        .await
        .unwrap();
    let tab = c.call("pane.get", json!({"pane_id": a})).await.unwrap()["pane"]["tab_id"].clone();

    let exported = c
        .call("layout.export", json!({"tab_id": tab}))
        .await
        .unwrap();
    assert_eq!(exported["workspace_id"], json!(ws));
    let root = exported["root"].clone();
    assert_eq!(root["dir"], "down");
    let l = leaves(&root);
    assert_eq!(l[0]["pane_id"], json!(a));
    assert_eq!(l[0]["argv"], json!(["sleep", "30"]));
    assert_eq!(l[0]["cwd"], json!(cwd));
    assert_eq!(l[1]["name"], "worker");

    // Names must be free: the exported tree still names a live pane.
    let e = c
        .call("layout.apply", json!({"workspace_id": ws, "root": root}))
        .await
        .unwrap_err();
    assert!(e.starts_with("BAD_PARAMS"), "{e}");
    c.call("pane.rename", json!({"pane_id": b, "name": null}))
        .await
        .unwrap();

    let mut viewer = s.client().await;
    viewer
        .call(
            "subscribe",
            json!({"events": ["tab.created", "pane.created"]}),
        )
        .await
        .unwrap();
    let r = c
        .call(
            "layout.apply",
            json!({"workspace_id": ws, "title": "copy", "root": root}),
        )
        .await
        .unwrap();
    let new_tab = &r["tab"];
    assert_eq!(new_tab["title"], "copy");
    assert_ne!(new_tab["id"], tab);
    let applied = leaves(&new_tab["layout"]);
    assert_eq!(applied.len(), 2);
    assert_eq!(new_tab["layout"]["ratio"], root["ratio"]);
    let panes = r["panes"].as_array().unwrap();
    assert_eq!(panes[1]["pane"]["name"], "worker");
    assert_eq!(
        panes[1]["pane"]["argv"],
        json!(["sh", "-c", "echo hi; sleep 30"])
    );
    assert_eq!(applied[1]["pane_id"], panes[1]["pane"]["id"]);
    assert_eq!(new_tab["active_pane_id"], panes[0]["pane"]["id"]);
    let ev = viewer
        .read_events(3, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(ev[0]["event"], "tab.created");
    assert_eq!(ev[2]["event"], "pane.created");
    // The new panes really run.
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": "worker", "match": "hi", "timeout_s": 10}),
    )
    .await
    .unwrap();

    // A bare leaf starts the user shell in the workspace folder.
    let r = c
        .call(
            "layout.apply",
            json!({"workspace_id": ws, "root": {"type": "pane"}}),
        )
        .await
        .unwrap();
    assert_eq!(r["panes"][0]["pane"]["cwd"], json!(cwd));

    let before = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap()["tabs"]
        .as_array()
        .unwrap()
        .len();
    // A sleep only this test starts, to find its process afterwards.
    let marker = format!("{}.5", std::process::id());
    for (root, code) in [
        (json!({"type": "pane", "cwd": "/nonexistent"}), "BAD_PARAMS"),
        (json!({"type": "pane", "argv": []}), "BAD_PARAMS"),
        (json!({"type": "pane", "name": "Bad"}), "BAD_PARAMS"),
        (
            json!({"type": "split", "dir": "right", "ratio": 0.5,
                "first": {"type": "pane", "name": "dup"},
                "second": {"type": "pane", "name": "dup"}}),
            "BAD_PARAMS",
        ),
        // The first pane starts, the second fails: the first is undone.
        (
            json!({"type": "split", "dir": "right", "ratio": 0.5,
                "first": {"type": "pane", "argv": ["sleep", marker]},
                "second": {"type": "pane", "argv": ["/nonexistent/agent"]}}),
            "SPAWN_FAILED",
        ),
    ] {
        let e = c
            .call("layout.apply", json!({"workspace_id": ws, "root": root}))
            .await
            .unwrap_err();
        assert!(e.starts_with(code), "{e}");
    }
    // The started pane's process is gone too.
    let mut alive = true;
    for _ in 0..50 {
        let out = std::process::Command::new("pgrep")
            .args(["-f", &format!("^sleep {marker}$")])
            .output()
            .unwrap();
        alive = out.status.success();
        if !alive {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!alive, "the undone pane still runs");
    // Nothing was left behind by the failures.
    let after = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert_eq!(after["tabs"].as_array().unwrap().len(), before);
    let e = c
        .call("layout.export", json!({"tab_id": "tab_nope"}))
        .await
        .unwrap_err();
    assert!(e.starts_with("NO_SUCH_TAB"), "{e}");
}
