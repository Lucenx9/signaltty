//! Spec 038: `pane.rename` names a pane; the name works as its address.

use serde_json::json;
use signaltty_testkit::{TestClient, TestServer};

async fn spawn(c: &mut TestClient, ws: &str, script: &str) -> String {
    c.call(
        "pane.spawn",
        json!({"workspace_id": ws, "argv": ["sh", "-c", script]}),
    )
    .await
    .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn a_named_pane_is_addressable_by_name_and_survives_restart() {
    let mut s = TestServer::start().await;
    let mut c = s.client().await;
    let ws = c.call("workspace.create", json!({})).await.unwrap()["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let a = spawn(&mut c, &ws, "echo ready; sleep 30").await;
    let b = c
        .call("pane.split", json!({"pane_id": a, "argv": ["sleep", "30"]}))
        .await
        .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut viewer = s.client().await;
    viewer
        .call("subscribe", json!({"events": ["pane.updated"]}))
        .await
        .unwrap();
    let r = c
        .call("pane.rename", json!({"pane_id": a, "name": "reviewer"}))
        .await
        .unwrap();
    assert_eq!(r["pane"]["name"], "reviewer");
    let ev = viewer
        .read_events(1, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(ev[0]["payload"]["pane"]["name"], "reviewer");

    // The name stands in for the id in any pane-addressed method.
    let g = c
        .call("pane.get", json!({"pane_id": "reviewer"}))
        .await
        .unwrap();
    assert_eq!(g["pane"]["id"], json!(a));
    c.call(
        "pane.wait_for_output",
        json!({"pane_id": "reviewer", "match": "ready", "timeout_s": 10}),
    )
    .await
    .unwrap();
    // Handlers that read the raw request see the name resolved too.
    let r = c
        .call(
            "pane.input",
            json!({"pane_id": "reviewer", "data_b64": "eAo="}),
        )
        .await
        .unwrap();
    assert_eq!(r["written"], 2);
    c.call("pane.rename", json!({"pane_id": b, "name": "worker-1"}))
        .await
        .unwrap();
    c.call(
        "pane.move",
        json!({"pane_id": "worker-1", "target_pane_id": "reviewer"}),
    )
    .await
    .unwrap();
    // Layout leaves take names too.
    let tab = g["pane"]["tab_id"].clone();
    c.call(
        "tab.set_layout",
        json!({"tab_id": tab, "layout": {"type": "split", "dir": "down", "ratio": 0.3,
            "first": {"type": "pane", "pane_id": "worker-1"},
            "second": {"type": "pane", "pane_id": a}}}),
    )
    .await
    .unwrap();
    // Renaming by name and to the same name is fine.
    c.call(
        "pane.rename",
        json!({"pane_id": "reviewer", "name": "reviewer"}),
    )
    .await
    .unwrap();

    for (params, code) in [
        (json!({"pane_id": b, "name": "reviewer"}), "BAD_PARAMS"),
        (json!({"pane_id": b, "name": "Reviewer"}), "BAD_PARAMS"),
        (json!({"pane_id": b, "name": "1st"}), "BAD_PARAMS"),
        (json!({"pane_id": b, "name": "a".repeat(33)}), "BAD_PARAMS"),
        (json!({"pane_id": "nobody", "name": "x"}), "NO_SUCH_PANE"),
    ] {
        let e = c.call("pane.rename", params).await.unwrap_err();
        assert!(e.starts_with(code), "{e}");
    }

    s.restart().await;
    let mut c = s.client().await;
    let g = c
        .call("pane.get", json!({"pane_id": "reviewer"}))
        .await
        .unwrap();
    assert_eq!(g["pane"]["id"], json!(a));

    // Clearing frees the name.
    let r = c
        .call("pane.rename", json!({"pane_id": a, "name": null}))
        .await
        .unwrap();
    assert!(r["pane"].get("name").is_none());
    let e = c
        .call("pane.get", json!({"pane_id": "reviewer"}))
        .await
        .unwrap_err();
    assert!(e.starts_with("NO_SUCH_PANE"), "{e}");
    c.call("pane.rename", json!({"pane_id": b, "name": "reviewer"}))
        .await
        .unwrap();
}
