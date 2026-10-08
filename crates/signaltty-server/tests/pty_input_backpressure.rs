use std::time::Duration;

use base64::Engine;
use serde_json::json;
use signaltty_testkit::{TempGitRepo, TestServer};

#[tokio::test]
async fn stalled_pane_input_keeps_other_panes_and_signals_available() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start_with_env(&[("TOKIO_WORKER_THREADS", "1")]).await;
    let mut c = srv.client().await;
    let ready = srv.state_dir.join("raw-ready");
    let release = srv.state_dir.join("release-reader");
    let signal_seen = srv.state_dir.join("signal-seen");
    let received = srv.state_dir.join("received-input");
    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path(), "name": "backpressure"}),
        )
        .await
        .unwrap();
    let child = "import os, signal, sys, time, tty\ntty.setraw(0)\nsignal.signal(signal.SIGUSR1, lambda *_: open(sys.argv[3], 'w').close())\nopen(sys.argv[1], 'w').close()\nwhile not os.path.exists(sys.argv[2]): time.sleep(0.005)\nwith open(sys.argv[4], 'wb', buffering=0) as f:\n while True: f.write(os.read(0, 4096))\n";
    let stalled = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws["workspace"]["id"],
            "argv": ["python3", "-c", child, ready, release, signal_seen, received]}),
        )
        .await
        .unwrap();
    let tab = c
        .call("tab.create", json!({"workspace_id": ws["workspace"]["id"]}))
        .await
        .unwrap();
    let other = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws["workspace"]["id"], "tab_id": tab["tab"]["id"], "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let stalled_id = stalled["pane"]["id"].as_str().unwrap().to_string();
    let other_id = other["pane"]["id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("child in raw mode, not consuming input");

    let mut blocked_client = srv.client().await;
    let blocked_id = stalled_id.clone();
    let blocked = tokio::spawn(async move {
        blocked_client
            .call(
                "pane.input",
                json!({"pane_id": blocked_id,
            "data_b64": base64::engine::general_purpose::STANDARD.encode(vec![b'x'; 256 * 1024])}),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !blocked.is_finished(),
        "fixture must saturate the raw PTY input buffer"
    );

    let mut queued_client = srv.client().await;
    let queued_id = stalled_id.clone();
    let queued = tokio::spawn(async move {
        queued_client
            .call(
                "pane.input",
                json!({"pane_id": queued_id,
            "data_b64": "Rk9MTE9XX1VQ"}),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !queued.is_finished(),
        "same-pane input must wait for its writer"
    );

    let status =
        tokio::time::timeout(Duration::from_secs(1), c.call("server.status", json!({}))).await;

    let mut other_client = srv.client().await;
    let other_input = tokio::time::timeout(
        Duration::from_secs(1),
        other_client.call(
            "pane.input",
            json!({"pane_id": other_id, "data_b64": "b3RoZXIK"}),
        ),
    )
    .await;
    let mut signal_client = srv.client().await;
    let signal = tokio::time::timeout(
        Duration::from_secs(1),
        signal_client.call(
            "pane.signal",
            json!({"pane_id": stalled_id, "signal": "USR1"}),
        ),
    )
    .await;
    let acknowledged = tokio::time::timeout(Duration::from_secs(1), async {
        while !signal_seen.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;

    // Release the old implementation's blocked write before assertions/teardown.
    std::fs::write(&release, b"release").unwrap();
    let unblocked = tokio::time::timeout(Duration::from_secs(3), blocked).await;
    let queued_result = tokio::time::timeout(Duration::from_secs(3), queued).await;
    let drained = tokio::time::timeout(Duration::from_secs(2), async {
        while std::fs::read(&received).unwrap_or_default().len() < 256 * 1024 + 9 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    let bytes = std::fs::read(&received).unwrap_or_default();
    srv.shutdown().await;
    assert_eq!(
        unblocked
            .expect("input must settle after fixture release")
            .unwrap()
            .unwrap()["written"],
        256 * 1024
    );
    assert_eq!(queued_result.unwrap().unwrap().unwrap()["written"], 9);
    assert!(drained.is_ok(), "raw child must record all accepted bytes");
    assert_eq!(
        bytes,
        [vec![b'x'; 256 * 1024], b"FOLLOW_UP".to_vec()].concat(),
        "concurrent same-pane writes must not interleave"
    );
    assert!(
        status.unwrap().is_ok(),
        "store-only requests remain responsive"
    );
    assert_eq!(
        other_input
            .expect("a stalled pane must not block another pane's input")
            .unwrap()["written"],
        6
    );
    assert_eq!(
        signal
            .expect("a stalled pane must remain signalable")
            .unwrap()["sent"],
        true
    );
    assert!(
        acknowledged.is_ok(),
        "child must observe its signal before consuming input"
    );
}
