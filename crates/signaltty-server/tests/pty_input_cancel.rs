use std::time::Duration;

use base64::Engine;
use serde_json::json;
use signaltty_testkit::{TempGitRepo, TestServer};

#[tokio::test]
async fn pane_close_releases_stalled_input() {
    run_case("close").await;
}

#[tokio::test]
async fn child_death_releases_stalled_input() {
    run_case("death").await;
}

#[tokio::test]
async fn stalled_input_has_a_total_deadline() {
    run_case("timeout").await;
}

#[tokio::test]
async fn shutdown_exits_with_stalled_input() {
    run_case("shutdown").await;
}

async fn run_case(action: &str) {
    let repo = TempGitRepo::new();
    let mut srv = TestServer::start_with_env(&[("TOKIO_WORKER_THREADS", "1")]).await;
    let mut c = srv.client().await;
    let ready = srv.state_dir.join("raw-ready");
    let release = srv.state_dir.join("release");
    let drained = srv.state_dir.join("drained");
    let ws = c
        .call("workspace.create", json!({"cwd":repo.path()}))
        .await
        .unwrap();
    let child = "import os, sys, time, tty, select\ntty.setraw(0)\nopen(sys.argv[1], 'w').close()\nwhile not os.path.exists(sys.argv[2]): time.sleep(0.005)\ndata = bytearray()\nwhile select.select([0], [], [], 0.2)[0]: data.extend(os.read(0, 4096))\nopen(sys.argv[3], 'wb').write(data)\nwhile True: time.sleep(0.05)\n";
    let pane = c
        .call(
            "pane.spawn",
            json!({"workspace_id":ws["workspace"]["id"],
        "argv":["python3", "-c", child, ready, release, drained]}),
        )
        .await
        .unwrap();
    let pane_id = pane["pane"]["id"].as_str().unwrap().to_string();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !ready.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("raw child ready");
    let mut input_client = srv.client().await;
    let input_id = pane_id.clone();
    let input = tokio::spawn(async move {
        input_client
            .call_raw_resp(
                "pane.input",
                json!({"pane_id": input_id,
            "data_b64":base64::engine::general_purpose::STANDARD.encode(vec![b'x';256*1024])}),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!input.is_finished(), "fixture must saturate the PTY");
    if action == "shutdown" {
        let stopped = c.call("server.shutdown", json!({"force":true})).await;
        let exited = srv.wait_for_exit(Duration::from_secs(2)).await;
        srv.kill().await;
        assert_eq!(stopped.unwrap()["stopped"], true);
        assert!(
            exited,
            "shutdown must exit, not retain a blocked input worker"
        );
        return;
    }
    let changed = match action {
        "close" => tokio::time::timeout(
            Duration::from_secs(1),
            c.call("pane.close", json!({"pane_id":pane_id})),
        )
        .await
        .unwrap(),
        "death" => {
            c.call("pane.signal", json!({"pane_id":pane_id,"signal":"KILL"}))
                .await
        }
        _ => Ok(json!({})),
    };
    let settled = tokio::time::timeout(
        Duration::from_secs(if action == "timeout" { 6 } else { 1 }),
        input,
    )
    .await;
    changed.unwrap();
    let response = settled
        .expect("pane close must release the actual input operation")
        .unwrap()
        .unwrap();
    let error = response
        .error
        .expect("closed PTY cannot complete the input");
    assert_eq!(
        error.code,
        if action == "timeout" {
            signaltty_proto::code::TIMEOUT
        } else {
            signaltty_proto::code::PANE_EXITED
        }
    );
    let written = error.details["written_bytes"]
        .as_u64()
        .expect("partial byte evidence");
    assert!(written > 0 && written < 256 * 1024);
    if action == "timeout" {
        std::fs::write(&release, b"go").unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !drained.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("child drains the accepted prefix");
        assert_eq!(
            std::fs::read(&drained).unwrap(),
            vec![b'x'; written as usize]
        );
    }
    srv.kill().await;
}
