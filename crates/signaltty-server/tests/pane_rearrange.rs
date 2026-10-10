//! Spec 037: `pane.swap` and `pane.move` rearrange live panes.

use serde_json::{json, Value};
use signaltty_testkit::{TestClient, TestServer};

async fn ws(c: &mut TestClient) -> String {
    c.call("workspace.create", json!({})).await.unwrap()["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn spawn(c: &mut TestClient, ws: &str, tab: Option<&str>) -> String {
    c.call(
        "pane.spawn",
        json!({"workspace_id": ws, "tab_id": tab, "argv": ["sleep", "30"]}),
    )
    .await
    .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn split(c: &mut TestClient, pane: &str, dir: &str) -> String {
    c.call(
        "pane.split",
        json!({"pane_id": pane, "direction": dir, "argv": ["sleep", "30"]}),
    )
    .await
    .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn tab_of(c: &mut TestClient, pane: &str) -> String {
    c.call("pane.get", json!({"pane_id": pane})).await.unwrap()["pane"]["tab_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn tab(c: &mut TestClient, ws: &str, id: &str) -> Value {
    let g = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    g["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .unwrap()
        .clone()
}

fn leaves(layout: &Value) -> Vec<String> {
    match layout["type"].as_str() {
        Some("pane") => vec![layout["pane_id"].as_str().unwrap().to_string()],
        Some("split") => [leaves(&layout["first"]), leaves(&layout["second"])].concat(),
        _ => vec![],
    }
}

#[tokio::test]
async fn swap_exchanges_slots_within_and_across_tabs() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let w = ws(&mut c).await;
    let a = spawn(&mut c, &w, None).await;
    let b = split(&mut c, &a, "down").await;
    let t1 = tab_of(&mut c, &a).await;
    let t2 = c
        .call("tab.create", json!({"workspace_id": w}))
        .await
        .unwrap()["tab"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let x = spawn(&mut c, &w, Some(&t2)).await;
    let before = tab(&mut c, &w, &t1).await["layout"].clone();

    let mut viewer = s.client().await;
    viewer
        .call("subscribe", json!({"events": ["tab.updated"]}))
        .await
        .unwrap();
    let r = c
        .call("pane.swap", json!({"pane_id": a, "target_pane_id": b}))
        .await
        .unwrap();
    assert_eq!(leaves(&r["tabs"][0]["layout"]), vec![b.clone(), a.clone()]);
    let ev = viewer
        .read_events(1, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(ev[0]["event"], "tab.updated");
    // Shape and ratios are untouched.
    let after = tab(&mut c, &w, &t1).await["layout"].clone();
    assert_eq!(after["dir"], before["dir"]);
    assert_eq!(after["ratio"], before["ratio"]);

    // Across tabs each pane takes the other's slot and tab.
    c.call("pane.swap", json!({"pane_id": a, "target_pane_id": x}))
        .await
        .unwrap();
    assert_eq!(
        leaves(&tab(&mut c, &w, &t1).await["layout"]),
        vec![b.clone(), x.clone()]
    );
    assert_eq!(
        leaves(&tab(&mut c, &w, &t2).await["layout"]),
        vec![a.clone()]
    );
    assert_eq!(tab_of(&mut c, &a).await, t2);
    assert_eq!(tab_of(&mut c, &x).await, t1);
    assert_eq!(tab(&mut c, &w, &t2).await["active_pane_id"], json!(a));

    for (params, code) in [
        (json!({"pane_id": a, "target_pane_id": a}), "BAD_PARAMS"),
        (
            json!({"pane_id": a, "target_pane_id": "nope"}),
            "NO_SUCH_PANE",
        ),
    ] {
        let e = c.call("pane.swap", params).await.unwrap_err();
        assert!(e.starts_with(code), "{e}");
    }
    let other = ws(&mut c).await;
    let y = spawn(&mut c, &other, None).await;
    let e = c
        .call("pane.move", json!({"pane_id": a, "target_pane_id": y}))
        .await
        .unwrap_err();
    assert!(e.starts_with("BAD_PARAMS"), "{e}");
}

#[tokio::test]
async fn move_splits_next_to_the_target() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let w = ws(&mut c).await;
    let a = spawn(&mut c, &w, None).await;
    let b = split(&mut c, &a, "right").await;
    let cc = split(&mut c, &b, "right").await;
    let t1 = tab_of(&mut c, &a).await;

    // Same tab: a leaves the left edge and lands below c.
    c.call(
        "pane.move",
        json!({"pane_id": a, "target_pane_id": cc, "direction": "down"}),
    )
    .await
    .unwrap();
    let l = tab(&mut c, &w, &t1).await["layout"].clone();
    assert_eq!(leaves(&l), vec![b.clone(), cc.clone(), a.clone()]);
    assert_eq!(l["second"]["dir"], "down");

    // The last pane of a tab leaves it empty.
    let t2 = c
        .call("tab.create", json!({"workspace_id": w}))
        .await
        .unwrap()["tab"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let x = spawn(&mut c, &w, Some(&t2)).await;
    let mut viewer = s.client().await;
    viewer
        .call("subscribe", json!({"events": ["pane.updated"]}))
        .await
        .unwrap();
    let r = c
        .call("pane.move", json!({"pane_id": x, "target_pane_id": b}))
        .await
        .unwrap();
    assert_eq!(r["tabs"].as_array().unwrap().len(), 2);
    let ev = viewer
        .read_events(1, std::time::Duration::from_secs(5))
        .await;
    assert_eq!(ev[0]["payload"]["pane"]["tab_id"], json!(t1));
    let emptied = tab(&mut c, &w, &t2).await;
    assert!(emptied["layout"].is_null());
    assert!(emptied["active_pane_id"].is_null());
    assert_eq!(tab_of(&mut c, &x).await, t1);
    assert_eq!(
        leaves(&tab(&mut c, &w, &t1).await["layout"]),
        vec![b, x, cc, a]
    );
}
