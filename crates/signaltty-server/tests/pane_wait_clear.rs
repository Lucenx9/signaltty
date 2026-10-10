//! Spec 036: `pane.wait_for_output` and `pane.clear` over real PTYs.

use std::time::Duration;

use base64::Engine;
use serde_json::json;
use signaltty_testkit::{TestClient, TestServer};

async fn spawn(c: &mut TestClient, script: &str) -> String {
    let ws = c.call("workspace.create", json!({})).await.unwrap()["workspace"]["id"]
        .as_str()
        .unwrap()
        .to_string();
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

async fn wait_output(
    c: &mut TestClient,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    c.call("pane.wait_for_output", params).await
}

fn release_file() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("signaltty-036-{}", signaltty_core::new_pane_id()))
}

#[tokio::test]
async fn waits_for_a_substring_or_regex_line() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let pane = spawn(&mut c, "sleep 0.3; echo build ok 42; sleep 30").await;

    let r = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "ok 4", "timeout_s": 10}),
    )
    .await
    .unwrap();
    assert_eq!(r["matched_line"], json!("build ok 42"));
    assert!(r["text"].as_str().unwrap().contains("build ok 42"));

    let r = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": r"^build ok \d+$", "regex": true, "timeout_s": 10}),
    )
    .await
    .unwrap();
    assert_eq!(r["matched_line"], json!("build ok 42"));

    // Without `regex`, the pattern is a plain substring.
    let e = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": r"^build ok \d+$", "timeout_s": 1}),
    )
    .await
    .unwrap_err();
    assert!(e.starts_with("TIMEOUT"), "{e}");

    let e = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "(", "regex": true}),
    )
    .await
    .unwrap_err();
    assert!(e.starts_with("BAD_PARAMS"), "{e}");
    let e = wait_output(&mut c, json!({"pane_id": "nope", "match": "x"}))
        .await
        .unwrap_err();
    assert!(e.starts_with("NO_SUCH_PANE"), "{e}");
}

#[tokio::test]
async fn rendered_mode_only_matches_lines_after_the_cursor() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let release = release_file();
    let script = format!(
        "echo MARK one; while [ ! -e {0} ]; do sleep 0.05; done; echo MARK two; sleep 30",
        release.display()
    );
    let pane = spawn(&mut c, &script).await;
    wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "MARK one", "timeout_s": 10}),
    )
    .await
    .unwrap();
    let head = c
        .call("pane.read", json!({"pane_id": pane, "mode": "rendered"}))
        .await
        .unwrap()["next_seq"]
        .as_u64()
        .unwrap();

    // The line already on screen does not satisfy a cursor wait...
    let e = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "MARK", "mode": "rendered", "after_seq": head, "timeout_s": 1}),
    )
    .await
    .unwrap_err();
    assert!(e.starts_with("TIMEOUT"), "{e}");

    // ...but new output arriving while the wait polls does.
    let mut waiter = s.client().await;
    let wait = tokio::spawn(async move {
        wait_output(
            &mut waiter,
            json!({"pane_id": pane, "match": "MARK", "mode": "rendered", "after_seq": head, "timeout_s": 10}),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(&release, "").unwrap();
    let r = wait.await.unwrap().unwrap();
    let _ = std::fs::remove_file(&release);
    assert_eq!(r["matched_line"], json!("MARK two"));
    assert!(r["next_seq"].as_u64().unwrap() > head);
}

#[tokio::test]
async fn an_exited_pane_without_a_match_fails_fast() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let pane = spawn(&mut c, "echo bye").await;
    let start = std::time::Instant::now();
    let e = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "never", "timeout_s": 30}),
    )
    .await
    .unwrap_err();
    assert!(e.starts_with("PANE_EXITED"), "{e}");
    assert!(start.elapsed() < Duration::from_secs(10));

    // Output written before the exit still matches.
    let r = wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "bye", "timeout_s": 5}),
    )
    .await
    .unwrap();
    assert_eq!(r["matched_line"], json!("bye"));
}

#[tokio::test]
async fn clear_blanks_the_server_view_and_streams_to_viewers() {
    let s = TestServer::start().await;
    let mut c = s.client().await;
    let pane = spawn(
        &mut c,
        "for i in $(seq 1 60); do echo line $i; done; sleep 30",
    )
    .await;
    wait_output(
        &mut c,
        json!({"pane_id": pane, "match": "line 60", "timeout_s": 10}),
    )
    .await
    .unwrap();

    let mut viewer = s.client().await;
    let offset = viewer
        .call("pane.attach", json!({"pane_id": pane, "mark_seen": false}))
        .await
        .unwrap()["output_offset"]
        .as_u64()
        .unwrap();

    c.call("pane.clear", json!({"pane_id": pane}))
        .await
        .unwrap();

    for mode in ["tail", "screen"] {
        let r = c
            .call("pane.read", json!({"pane_id": pane, "mode": mode}))
            .await
            .unwrap();
        assert!(
            !r["text"].as_str().unwrap().contains("line"),
            "{mode} after clear: {r}"
        );
    }
    let events = viewer.read_events(1, Duration::from_secs(5)).await;
    assert_eq!(events[0]["event"], "pty.data");
    let data = base64::engine::general_purpose::STANDARD
        .decode(events[0]["payload"]["data_b64"].as_str().unwrap())
        .unwrap();
    assert_eq!(data, b"\x1b[H\x1b[2J\x1b[3J");
    assert_eq!(
        events[0]["payload"]["output_offset"].as_u64().unwrap(),
        offset + data.len() as u64
    );

    let e = c
        .call("pane.clear", json!({"pane_id": "nope"}))
        .await
        .unwrap_err();
    assert!(e.starts_with("NO_SUCH_PANE"), "{e}");
}
