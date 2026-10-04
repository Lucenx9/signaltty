//! US3 (orchestrator reads): rendered grid + incremental cursor over real
//! PTYs. Failing-first tests for T008 (lane B, READ half only).

use std::time::Duration;

use serde_json::json;
use signaltty_testkit::{TestClient, TestServer};

async fn new_shell(c: &mut TestClient, argv: Vec<&str>) -> (String, String) {
    let ws = c.call("workspace.create", json!({})).await.unwrap()["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let p = c
        .call("pane.spawn", json!({"workspace_id": ws, "argv": argv}))
        .await
        .unwrap();
    let pane = p["pane"]["id"].as_str().unwrap().to_string();
    (ws, pane)
}

async fn wait_for_text(c: &mut TestClient, pane: &str, marker: &str) {
    let start = std::time::Instant::now();
    loop {
        let r = c
            .call(
                "pane.read",
                json!({"pane_id": pane, "mode": "tail", "lines": 50}),
            )
            .await
            .unwrap();
        let text = r["text"].as_str().unwrap_or("");
        if text.contains(marker) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {marker:?}; last tail: {text:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn rendered(c: &mut TestClient, pane: &str, after: u64, lines: u64) -> serde_json::Value {
    c.call(
        "pane.read",
        json!({"pane_id": pane, "mode": "rendered", "after_seq": after, "lines": lines}),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn rendered_reads_plain_output_incrementally() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let (_ws, pane) = new_shell(&mut c, vec!["sh", "-c", "echo alpha; echo beta; sleep 30"]).await;
    wait_for_text(&mut c, &pane, "beta").await;

    let r1 = rendered(&mut c, &pane, 0, 200).await;
    let t1 = r1["text"].as_str().unwrap();
    assert!(t1.contains("alpha"), "first read: {t1:?}");
    assert!(t1.contains("beta"), "first read: {t1:?}");
    assert_eq!(r1["dropped"], json!(false));
    let head = r1["next_seq"].as_u64().unwrap();

    // Delta-only second read: empty text, unchanged head.
    let r2 = rendered(&mut c, &pane, head, 200).await;
    assert_eq!(r2["text"], json!(""));
    assert_eq!(r2["next_seq"], json!(head));
    assert_eq!(r2["dropped"], json!(false));

    // New output after the cursor is returned exactly once.
    c.call(
        "pane.input",
        json!({"pane_id": pane, "data_b64": "echo gamma\n"}),
    )
    .await
    .unwrap();
    wait_for_text(&mut c, &pane, "gamma").await;
    let r3 = rendered(&mut c, &pane, head, 200).await;
    let t3 = r3["text"].as_str().unwrap();
    assert!(t3.contains("gamma"), "delta read: {t3:?}");
    assert!(!t3.contains("alpha"), "must not repeat old lines: {t3:?}");
}

#[tokio::test]
async fn rendered_resolves_tui_repaints_and_matches_tail() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let (_ws, pane) = new_shell(
        &mut c,
        vec![
            "sh",
            "-c",
            "printf 'Status: Starting\\rStatus: Ready\\nfirst\\nsecond\\n\\033[1A\\033[2Ksecond!\\n'; sleep 30",
        ],
    )
    .await;
    wait_for_text(&mut c, &pane, "second!").await;

    let r = rendered(&mut c, &pane, 0, 200).await;
    let text = r["text"].as_str().unwrap();
    assert!(
        text.contains("Status: Ready"),
        "repaint must resolve: {text:?}"
    );
    assert!(
        !text.contains("Starting"),
        "overwritten text must be gone: {text:?}"
    );
    assert!(text.contains("second!"), "cursor-up redraw: {text:?}");

    let tail = c
        .call(
            "pane.read",
            json!({"pane_id": pane, "mode": "tail", "lines": 200}),
        )
        .await
        .unwrap();
    assert_eq!(
        tail["text"].as_str().unwrap(),
        text,
        "tail must equal rendered on repaint fixtures"
    );
}

#[tokio::test]
async fn rendered_reports_dropped_after_eviction() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let (_ws, pane) = new_shell(&mut c, vec!["sh", "-c", "seq 1 6000; sleep 30"]).await;
    wait_for_text(&mut c, &pane, "6000").await;

    let stale = rendered(&mut c, &pane, 1, 5000).await;
    assert_eq!(
        stale["dropped"],
        json!(true),
        "evicted cursor must report dropped"
    );
    let fresh = rendered(&mut c, &pane, 0, 10).await;
    assert_eq!(fresh["dropped"], json!(false));
    assert_eq!(fresh["truncated"], json!(true));
    assert_eq!(fresh["text"].as_str().unwrap().lines().count(), 10);
}

#[tokio::test]
async fn rendered_alt_screen_returns_current_grid() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    // Pane that stays on the alt screen: current grid is the alt grid.
    let (_ws, alt) = new_shell(
        &mut c,
        vec![
            "sh",
            "-c",
            "printf 'main-output\\n\\033[?1049hALT-VIEW\\n'; sleep 30",
        ],
    )
    .await;
    wait_for_text(&mut c, &alt, "ALT-VIEW").await;
    let r = rendered(&mut c, &alt, 0, 200).await;
    let text = r["text"].as_str().unwrap();
    assert!(
        text.contains("ALT-VIEW"),
        "alt grid must be returned: {text:?}"
    );

    // Pane that entered and left alt: main grid restored, no alt residue.
    let (_ws2, back) = new_shell(
        &mut c,
        vec![
            "sh",
            "-c",
            "printf 'AAA\\n\\033[?1049hBBB\\n\\033[?1049lCCC\\n'; sleep 30",
        ],
    )
    .await;
    wait_for_text(&mut c, &back, "CCC").await;
    let r2 = rendered(&mut c, &back, 0, 200).await;
    let t2 = r2["text"].as_str().unwrap();
    assert!(t2.contains("AAA"), "main grid: {t2:?}");
    assert!(t2.contains("CCC"), "main grid: {t2:?}");
    assert!(!t2.contains("BBB"), "alt lines must not leak: {t2:?}");
}

#[tokio::test]
async fn rendered_unknown_pane_is_no_such_pane() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let err = c
        .call(
            "pane.read",
            json!({"pane_id": "pane_nope", "mode": "rendered"}),
        )
        .await
        .unwrap_err();
    assert!(err.starts_with("NO_SUCH_PANE"), "got: {err}");
}

#[tokio::test]
async fn rendered_old_cursor_dropped_after_restart() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let (_ws, pane) = new_shell(&mut c, vec!["sh", "-c", "echo persist-me; sleep 30"]).await;
    wait_for_text(&mut c, &pane, "persist-me").await;
    let r1 = rendered(&mut c, &pane, 0, 200).await;
    let head = r1["next_seq"].as_u64().unwrap();
    assert!(head > 0);

    drop(c);
    s.restart().await;
    let mut c2 = s.client().await;
    let r2 = rendered(&mut c2, &pane, head, 200).await;
    assert_eq!(r2["dropped"], json!(true), "pre-restart cursor must drop");
    let fresh = rendered(&mut c2, &pane, 0, 200).await;
    assert_eq!(fresh["dropped"], json!(false));
    assert!(
        fresh["text"].as_str().unwrap().contains("persist-me"),
        "restored tail must survive: {:?}",
        fresh["text"]
    );
}
