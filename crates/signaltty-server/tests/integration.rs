//! Phase 1 integration tests with real PTYs (§23):
//! spawn/read/exit, detach/reattach, resize, signals, malformed IPC,
//! notifications/attention, concurrent clients, splits, restart/restore.

use std::time::Duration;

use serde_json::json;
use signaltty_testkit::{TestClient, TestServer};

async fn new_pane(c: &mut TestClient, argv: Vec<&str>) -> (String, String) {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap().to_string();
    let p = c
        .call("pane.spawn", json!({"workspace_id": ws, "argv": argv}))
        .await
        .unwrap();
    (ws, p["pane"]["id"].as_str().unwrap().to_string())
}

async fn wait_for_text(c: &mut TestClient, pane: &str, marker: &str, timeout: Duration) {
    let start = std::time::Instant::now();
    loop {
        let r = c
            .call(
                "pane.read",
                json!({"pane_id": pane, "mode": "tail", "lines": 50}),
            )
            .await
            .unwrap();
        if r["text"].as_str().unwrap_or("").contains(marker) {
            return;
        }
        assert!(
            start.elapsed() < timeout,
            "timed out waiting for {marker:?}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn spawn_echo_and_read() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sh", "-c", "echo hello-signaltty"]).await;
    wait_for_text(&mut c, &pane, "hello-signaltty", Duration::from_secs(5)).await;
    // Process exits; wait observes it.
    let w = c
        .call(
            "wait",
            json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(w["satisfied"], true);
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["live"]["state"], "exited");
    assert_eq!(p["pane"]["live"]["code"], 0);
    // Generic one-shot exit 0 maps to done so wait --until done can observe it.
    assert_eq!(p["pane"]["lifecycle"], "done");
    srv.shutdown().await;
}

#[tokio::test]
async fn wait_until_done_observes_fast_one_shot_exit() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sh", "-c", "exit 0"]).await;
    let w = c
        .call(
            "wait",
            json!({"pane_id": pane, "until": "done", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(w["satisfied"], true);
    assert_eq!(w["outcome"], "done");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["live"]["state"], "exited");
    assert_eq!(p["pane"]["live"]["code"], 0);
    assert_eq!(p["pane"]["lifecycle"], "done");
    srv.shutdown().await;
}

#[tokio::test]
async fn input_roundtrip_and_exit_code() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(
        &mut c,
        vec!["sh", "-c", "read line; echo got:$line; exit 7"],
    )
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let data = base64_encode("ping\n");
    let r = c
        .call("pane.input", json!({"pane_id": pane, "data_b64": data}))
        .await
        .unwrap();
    assert_eq!(r["written"], 5);
    wait_for_text(&mut c, &pane, "got:ping", Duration::from_secs(5)).await;
    c.call(
        "wait",
        json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["live"]["code"], 7);
    srv.shutdown().await;
}

fn base64_encode(s: &str) -> String {
    // Minimal base64 to avoid adding deps to the test target.
    const ALPH: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = s.as_bytes();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut n = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            n |= (b as u32) << (16 - 8 * i);
        }
        let pad = 3 - chunk.len();
        for i in 0..4 - pad {
            out.push(ALPH[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in 0..pad {
            out.push('=');
        }
    }
    out
}

#[tokio::test]
async fn detach_reattach_keeps_process() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sh", "-c", "echo first; sleep 30"]).await;
    wait_for_text(&mut c, &pane, "first", Duration::from_secs(5)).await;
    // Attach (snapshot), then drop the client = crash/disconnect.
    let a = c
        .call("pane.attach", json!({"pane_id": pane}))
        .await
        .unwrap();
    assert!(a.get("snapshot_b64").is_some());
    drop(c);
    // New client: same pane, still live, history intact.
    let mut c2 = srv.client().await;
    let p = c2.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["live"]["state"], "live");
    let r = c2
        .call(
            "pane.read",
            json!({"pane_id": pane, "mode": "tail", "lines": 10}),
        )
        .await
        .unwrap();
    assert!(r["text"].as_str().unwrap().contains("first"));
    srv.shutdown().await;
}

#[tokio::test]
async fn resize_and_signal() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let r = c
        .call(
            "pane.resize",
            json!({"pane_id": pane, "cols": 100, "rows": 30}),
        )
        .await
        .unwrap();
    assert_eq!(r["pane"]["pty_size"]["cols"], 100);
    // Out-of-range clamps instead of failing.
    let r = c
        .call(
            "pane.resize",
            json!({"pane_id": pane, "cols": 5000, "rows": 1}),
        )
        .await
        .unwrap();
    assert_eq!(r["pane"]["pty_size"]["cols"], 500);
    c.call("pane.signal", json!({"pane_id": pane, "signal": "TERM"}))
        .await
        .unwrap();
    c.call(
        "wait",
        json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
    )
    .await
    .unwrap();
    // Signalling a dead pane reports PANE_EXITED.
    let err = c
        .call("pane.signal", json!({"pane_id": pane, "signal": "INT"}))
        .await
        .unwrap_err();
    assert!(err.starts_with("PANE_EXITED"), "{err}");
    srv.shutdown().await;
}

#[tokio::test]
async fn malformed_ipc_and_versioning() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    // Invalid JSON.
    let resp: serde_json::Value =
        serde_json::from_str(c.raw_roundtrip("not json").await.trim()).unwrap();
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["error"]["code"], "BAD_PARAMS");
    // Wrong protocol.
    let resp: serde_json::Value = serde_json::from_str(
        c.raw_roundtrip(
            r#"{"protocol":"signaltty/99","id":"1","method":"server.status","params":{}}"#,
        )
        .await
        .trim(),
    )
    .unwrap();
    assert_eq!(resp["error"]["code"], "BAD_PROTOCOL");
    // Unknown method.
    let err = c.call("nope.nope", json!({})).await.unwrap_err();
    assert!(err.starts_with("UNKNOWN_METHOD"), "{err}");
    // Missing resource.
    let err = c
        .call("pane.get", json!({"pane_id": "pane_missing"}))
        .await
        .unwrap_err();
    assert!(err.starts_with("NO_SUCH_PANE"), "{err}");
    // Server survives all of it.
    assert!(c.call("server.status", json!({})).await.is_ok());
    srv.shutdown().await;
}

#[tokio::test]
async fn notify_attention_and_next_unread() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    // Subscribe BEFORE notifying.
    let mut sub = srv.client().await;
    sub.call(
        "subscribe",
        json!({"events": ["notification.*", "attention.*"]}),
    )
    .await
    .unwrap();
    c.call(
        "notify",
        json!({"pane_id": pane, "title": "Codex needs input", "body": "review?", "severity": "error"}),
    )
    .await
    .unwrap();
    // Drain two events: notification.created + attention.created.
    let evs = drain_events(&mut sub, 2, Duration::from_secs(5)).await;
    let names: Vec<_> = evs
        .iter()
        .map(|e| e["event"].as_str().unwrap().to_string())
        .collect();
    assert!(
        names.contains(&"notification.created".to_string()),
        "{names:?}"
    );
    assert!(
        names.contains(&"attention.created".to_string()),
        "{names:?}"
    );
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["attention"], "error");
    assert!(p["pane"]["last_message"]
        .as_str()
        .unwrap()
        .contains("Codex"));
    // Lifecycle untouched by attention.
    assert_eq!(p["pane"]["lifecycle"], "unknown");
    // next_unread points here.
    let f = c.call("focus.next_unread", json!({})).await.unwrap();
    assert_eq!(f["pane_id"], pane);
    // mark_seen clears attention only.
    c.call("pane.mark_seen", json!({"pane_id": pane}))
        .await
        .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["attention"], "none");
    let f = c.call("focus.next_unread", json!({})).await.unwrap();
    assert!(f["pane_id"].is_null());
    srv.shutdown().await;
}

async fn drain_events(
    c: &mut TestClient,
    count: usize,
    timeout: Duration,
) -> Vec<serde_json::Value> {
    // TestClient::call skips events, so subscribe_collect must be used on a
    // fresh subscription; here we re-subscribe and wait for NEW events only
    // if count not yet satisfied. Simpler: use raw reads via a second helper.
    // We already subscribed; read raw lines directly.
    c.read_events(count, timeout).await
}

#[tokio::test]
async fn osc777_from_pty_raises_notification() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(
        &mut c,
        vec![
            "sh",
            "-c",
            r"printf '\033]777;notify;Build;done-ok\007'; sleep 30",
        ],
    )
    .await;
    // Poll until attention flips.
    let start = std::time::Instant::now();
    loop {
        let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
        if p["pane"]["attention"] == "unread" {
            assert!(p["pane"]["last_message"]
                .as_str()
                .unwrap()
                .contains("done-ok"));
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no OSC attention");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    srv.shutdown().await;
}

#[tokio::test]
async fn split_layout_and_concurrent_clients() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    // Second client splits concurrently.
    let mut c2 = srv.client().await;
    let s = c2
        .call(
            "pane.split",
            json!({"pane_id": pane, "direction": "down", "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    let pane2 = s["pane"]["id"].as_str().unwrap().to_string();
    assert_ne!(pane, pane2);
    // Layout contains both.
    let g = c.call("pane.get", json!({"pane_id": pane2})).await.unwrap();
    let tab_id = g["pane"]["tab_id"].as_str().unwrap().to_string();
    let _ = tab_id;
    // Both live from either client's view.
    let st = c.call("server.status", json!({})).await.unwrap();
    assert_eq!(st["live_panes"], 2);
    let st2 = c2.call("server.status", json!({})).await.unwrap();
    assert_eq!(st2["live_panes"], 2);
    srv.shutdown().await;
}

#[tokio::test]
async fn restart_restores_structure_without_processes() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sh", "-c", "echo persist-me; sleep 30"]).await;
    wait_for_text(&mut c, &pane, "persist-me", Duration::from_secs(5)).await;
    c.call(
        "report-session",
        json!({"pane_id": pane, "agent_session_id": "sess-123", "agent": "codex"}),
    )
    .await
    .unwrap();
    c.call(
        "notify",
        json!({"pane_id": pane, "title": "t", "body": "b"}),
    )
    .await
    .unwrap();
    let pane_id = pane.clone();
    drop(c);
    srv.restart().await;
    let mut c = srv.client().await;
    let p = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    // Honest restore: no process, structure + metadata + tail kept.
    assert_eq!(p["pane"]["live"]["state"], "exited");
    assert!(p["pane"]["restore_state"] == "RESTORED" || p["pane"]["restore_state"] == "RESUMABLE");
    assert_eq!(p["pane"]["agent"]["agent_session_id"], "sess-123");
    assert_eq!(p["pane"]["agent"]["kind"], "codex");
    assert_eq!(p["pane"]["attention"], "unread"); // unread stays unread
    let r = c
        .call(
            "pane.read",
            json!({"pane_id": pane_id, "mode": "tail", "lines": 10}),
        )
        .await
        .unwrap();
    assert!(r["text"].as_str().unwrap().contains("persist-me"));
    srv.shutdown().await;
}

fn decision_payload(id: &str) -> serde_json::Value {
    json!({
        "id": id, "prompt": "Allow rm -rf /tmp/x?",
        "options": [
            {"id": "once", "label": "Once"},
            {"id": "always", "label": "Always"},
            {"id": "deny", "label": "Deny"},
        ],
    })
}

#[tokio::test]
async fn decision_answer_delivers_bytes_and_consumes() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    // A codex pane whose child is `cat`: echoes whatever the answer
    // channel types (`agent_hint` is how tests stand in for the real CLI).
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "argv": ["cat"], "agent_hint": "codex"}),
        )
        .await
        .unwrap();
    let pane = p["pane"]["id"].as_str().unwrap().to_string();
    tokio::time::sleep(Duration::from_millis(200)).await;

    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "decision": decision_payload("d1")}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d1");
    assert_eq!(p["pane"]["pending_decision"]["answerable"], true);
    assert_eq!(
        p["pane"]["pending_decision"]["options"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // Answering "Once" types "1\\n": cat echoes it back.
    let r = c
        .call(
            "decision.answer",
            json!({"pane_id": pane, "decision_id": "d1", "option_id": "once"}),
        )
        .await
        .unwrap();
    assert_eq!(r["answered"], true);
    assert_eq!(r["attention"], "none");
    assert_eq!(r["lifecycle"], "blocked");
    wait_for_text(&mut c, &pane, "1", Duration::from_secs(5)).await;

    // Consumed: the bar is gone and a repeat never redelivers.
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert!(p["pane"].get("pending_decision").is_none());
    assert_eq!(p["pane"]["attention"], "none");
    assert_eq!(p["pane"]["lifecycle"], "blocked");
    let err = c
        .call(
            "decision.answer",
            json!({"pane_id": pane, "decision_id": "d1", "option_id": "once"}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("NO_SUCH_DECISION"), "{err}");
    srv.shutdown().await;
}

#[tokio::test]
async fn decision_prose_supersede_and_clearing() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;

    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "decision": decision_payload("d1")}),
    )
    .await
    .unwrap();
    // Prose-only attention neither fakes buttons nor drops the real ones.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "message": "approval requested"}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d1");

    // Newer decision supersedes: latest wins.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "decision": decision_payload("d2")}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d2");
    // The superseded id is stale, never redeliverable.
    let err = c
        .call(
            "decision.answer",
            json!({"pane_id": pane, "decision_id": "d1", "option_id": "once"}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("NO_SUCH_DECISION"), "{err}");

    // The agent moving on (out of blocked) drops the bar.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "UserPromptSubmit", "pane_id": pane}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert!(p["pane"].get("pending_decision").is_none());

    // Reading an approval preserves its decision and required attention.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "decision": decision_payload("d3")}),
    )
    .await
    .unwrap();
    c.call("pane.mark_seen", json!({"pane_id": pane}))
        .await
        .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d3");
    assert_eq!(p["pane"]["attention"], "permission_required");
    assert!(p["pane"]["last_seen_at"].is_string());
    let attached = c
        .call("pane.attach", json!({"pane_id": pane}))
        .await
        .unwrap();
    assert_eq!(attached["attention"], "permission_required");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d3");

    // Unknown option is a shape error; the decision survives it.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane,
               "decision": decision_payload("d4")}),
    )
    .await
    .unwrap();
    let err = c
        .call(
            "decision.answer",
            json!({"pane_id": pane, "decision_id": "d4", "option_id": "maybe"}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d4");

    // Exiting the child drops the bar with it.
    c.call("pane.signal", json!({"pane_id": pane, "signal": "KILL"}))
        .await
        .unwrap();
    c.call(
        "wait",
        json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert!(p["pane"].get("pending_decision").is_none());
    srv.shutdown().await;
}

#[tokio::test]
async fn decision_without_channel_is_read_only() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "Notification", "pane_id": pane,
               "decision": decision_payload("d1")}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["answerable"], false);
    // Server refuses to guess a delivery channel; the decision survives.
    let err = c
        .call(
            "decision.answer",
            json!({"pane_id": pane, "decision_id": "d1", "option_id": "once"}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    assert!(err.contains("no answer channel"), "{err}");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["pending_decision"]["id"], "d1");
    srv.shutdown().await;
}

fn write_manifest(dir: &std::path::Path, name: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(name), body).unwrap();
}

#[tokio::test]
async fn manifest_overlay_detects_and_classifies() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-agents-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_manifest(
        &dir,
        "wrap.toml",
        r#"
[agent]
kind = "codex"
binaries = ["sleep"]

[lifecycle.PingTest]
lifecycle = "working"
message = "future says hi"
"#,
    );
    // Malformed files never block startup (isolated, logged, skipped).
    write_manifest(&dir, "broken.toml", "[agent\nkind = ");
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;

    // Spawn-time detection honors overlay binaries.
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "codex");

    // Overlay hook maps classify; unmapped hooks fall through to builtin.
    let r = c
        .call(
            "hook-event",
            json!({"agent": "codex", "event": "PingTest", "pane_id": pane}),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "working");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["last_message"], "future says hi");
    let r = c
        .call(
            "hook-event",
            json!({"agent": "codex", "event": "Stop", "pane_id": pane}),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "done");
    srv.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn procscan_promotes_through_shell_and_follows_cwd() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-agents-proc-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_manifest(
        &dir,
        "wrap.toml",
        "[agent]\nkind = \"codex\"\nbinaries = [\"sleep\"]\n",
    );
    let srv = TestServer::start_with_dirs(None, Some(&dir)).await;
    let mut c = srv.client().await;
    // Spawned as `sh` (generic); the tick sees through to `sleep`.
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "argv": ["sh", "-c", "cd /; exec sleep 30"]}),
        )
        .await
        .unwrap();
    let pane = p["pane"]["id"].as_str().unwrap().to_string();
    // Promotion + cwd follow land within two ticks.
    let start = std::time::Instant::now();
    loop {
        let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
        if p["pane"]["agent"]["kind"] == "codex" && p["pane"]["cwd"] == "/" {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(25),
            "no promotion: {p}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    // Afterwards the tick is quiet: nothing pushed for a full interval.
    let mut sub = srv.client().await;
    sub.call("subscribe", json!({"events": ["pane.*"]}))
        .await
        .unwrap();
    let heard = tokio::time::timeout(
        Duration::from_secs(11),
        sub.read_events(1, Duration::from_secs(30)),
    )
    .await;
    assert!(heard.is_err(), "tick must be quiet, got {heard:?}");
    srv.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn workspace_handles_collide_resolve_and_migrate() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    // Same name twice → my-api, my-api-2.
    let a = c
        .call(
            "workspace.create",
            json!({"name": "My API!!", "cwd": "/tmp"}),
        )
        .await
        .unwrap();
    let b = c
        .call(
            "workspace.create",
            json!({"name": "My API!!", "cwd": "/tmp"}),
        )
        .await
        .unwrap();
    assert_eq!(a["workspace"]["handle"], "my-api");
    assert_eq!(b["workspace"]["handle"], "my-api-2");
    // Handle-or-id resolves everywhere an id does.
    let g = c
        .call("workspace.get", json!({"workspace_id": "my-api"}))
        .await
        .unwrap();
    assert_eq!(g["workspace"]["name"], "My API!!");
    c.call(
        "workspace.rename",
        json!({"workspace_id": "my-api", "name": "Renamed"}),
    )
    .await
    .unwrap();
    let g = c
        .call("workspace.get", json!({"workspace_id": "my-api"}))
        .await
        .unwrap();
    assert_eq!(g["workspace"]["name"], "Renamed");
    assert_eq!(g["workspace"]["handle"], "my-api", "handles are immutable");
    c.call(
        "tab.create",
        json!({"workspace_id": "my-api-2", "title": "t"}),
    )
    .await
    .unwrap();
    c.call("workspace.refresh_git", json!({"workspace_id": "my-api-2"}))
        .await
        .unwrap();
    let err = c
        .call("workspace.get", json!({"workspace_id": "nope"}))
        .await
        .unwrap_err();
    assert!(err.contains("NO_SUCH_WORKSPACE"), "{err}");
    // Legacy migration: strip handles from the snapshot, restart, backfilled.
    drop(c);
    srv.restart().await;
    let snap_path = srv.state_dir.join("snapshot.json");
    // Graceful shutdown writes the snapshot; force a legacy shape by
    // removing every handle, then restart once more.
    let raw = std::fs::read_to_string(&snap_path).unwrap();
    let mut snap: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for ws in snap["workspaces"].as_array_mut().unwrap() {
        ws.as_object_mut().unwrap().remove("handle");
    }
    std::fs::write(&snap_path, serde_json::to_string(&snap).unwrap()).unwrap();
    srv.restart().await;
    let mut c = srv.client().await;
    let l = c.call("workspace.list", json!({})).await.unwrap();
    let handles: Vec<String> = l["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["handle"].as_str().unwrap_or("").to_string())
        .collect();
    assert_eq!(handles.len(), 2);
    assert!(handles.iter().all(|h| !h.is_empty()), "{handles:?}");
    assert!(handles[0] != handles[1], "unique: {handles:?}");
    // Migrated handles resolve too.
    let g = c
        .call("workspace.get", json!({"workspace_id": handles[0].clone()}))
        .await
        .unwrap();
    assert_eq!(g["workspace"]["handle"], handles[0]);
    srv.shutdown().await;
}

#[tokio::test]
async fn audit_replays_across_restart() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let before = c.call("server.status", json!({})).await.unwrap()["seq"]
        .as_u64()
        .unwrap();
    c.call("notify", json!({"pane_id": pane, "title": "audit-me"}))
        .await
        .unwrap();
    drop(c);
    srv.restart().await;
    // The ring is empty after restart; the audit still serves the replay
    // for a pre-restart seq.
    let mut c2 = srv.client().await;
    c2.call("subscribe", json!({"events": ["*"], "from_seq": before}))
        .await
        .unwrap();
    let replayed = c2.read_events(2, Duration::from_secs(10)).await;
    let titles: Vec<&str> = replayed
        .iter()
        .filter(|e| e["event"] == "notification.created")
        .filter_map(|e| e["payload"]["notification"]["title"].as_str())
        .collect();
    assert_eq!(titles, vec!["audit-me"]);
    srv.shutdown().await;
}

#[tokio::test]
async fn new_events_after_restart_advance_the_previous_cursor() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "notify",
        json!({"pane_id": pane, "title": "before-restart"}),
    )
    .await
    .unwrap();
    let before = c.call("server.status", json!({})).await.unwrap()["seq"]
        .as_u64()
        .unwrap();
    drop(c);
    srv.restart().await;
    let mut c = srv.client().await;
    c.call("notify", json!({"pane_id": pane, "title": "after-restart"}))
        .await
        .unwrap();
    let after = c.call("server.status", json!({})).await.unwrap()["seq"]
        .as_u64()
        .unwrap();
    // Clean up the owned server even if the regression fails.
    let mut sub = srv.client().await;
    sub.call(
        "subscribe",
        json!({"events":["notification.created"], "from_seq":before}),
    )
    .await
    .unwrap();
    let replay = tokio::time::timeout(
        Duration::from_secs(1),
        sub.read_events(1, Duration::from_secs(2)),
    )
    .await;
    srv.shutdown().await;
    assert!(
        after > before,
        "new cursor {after} must advance old cursor {before}"
    );
    let replay = replay.expect("new event must replay after the old cursor");
    assert_eq!(
        replay[0]["payload"]["notification"]["title"],
        "after-restart"
    );
}

#[tokio::test]
async fn workspace_diff_reports_numstat_and_rejects_non_repo() {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-intdiff-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // The temp root is the git ceiling so a TMPDIR inside a checkout is never
    // discovered as a parent repo; passed explicitly, never via set_var.
    let ceiling = std::env::temp_dir();
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .env("GIT_CEILING_DIRECTORIES", &ceiling)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@t"]);
    run(&["config", "user.name", "t"]);
    run(&["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-qm", "base"]);
    std::fs::write(dir.join("a.txt"), "1\n2\n").unwrap();
    std::fs::write(dir.join("new.txt"), "u\n").unwrap();

    let srv =
        TestServer::start_with_env(&[("GIT_CEILING_DIRECTORIES", ceiling.to_str().unwrap())]).await;
    let mut c = srv.client().await;
    let w = c
        .call(
            "workspace.create",
            json!({"name": "diff ws", "cwd": dir.to_str().unwrap()}),
        )
        .await
        .unwrap();
    // Handle-or-id resolves for diff too.
    let d = c
        .call("workspace.diff", json!({"workspace_id": "diff-ws"}))
        .await
        .unwrap();
    assert_eq!(d["workspace_id"], w["workspace"]["id"]);
    let files = d["files"].as_array().unwrap();
    let a = files.iter().find(|f| f["path"] == "a.txt").unwrap();
    assert_eq!(
        (a["added"].as_u64(), a["removed"].as_u64()),
        (Some(1), Some(0))
    );
    let new = files.iter().find(|f| f["path"] == "new.txt").unwrap();
    assert_eq!(new["untracked"], true);
    assert_eq!(d["added"], 1);
    // Non-repo workspace is a loud error, never an empty lie. A sibling
    // dir, with the temp root as git ceiling so a TMPDIR inside a checkout
    // is never discovered as a parent repo.
    let bare = dir.with_extension("bare");
    std::fs::create_dir_all(&bare).unwrap();
    let w2 = c
        .call("workspace.create", json!({"cwd": bare.to_str().unwrap()}))
        .await
        .unwrap();
    let err = c
        .call(
            "workspace.diff",
            json!({"workspace_id": w2["workspace"]["id"]}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    srv.shutdown().await;
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&bare).ok();
}

#[tokio::test]
async fn hook_event_drives_codex_lifecycle() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;

    // SessionStart → idle, records session id + resume argv.
    let r = c
        .call(
            "hook-event",
            json!({
                "agent": "codex", "event": "SessionStart", "pane_id": pane,
                "payload": {"session_id": "sess-1", "cwd": "/tmp"}
            }),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "idle");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["agent"]["agent_session_id"], "sess-1");
    assert_eq!(p["pane"]["agent"]["kind"], "codex");
    assert_eq!(
        p["pane"]["agent"]["resume_argv"],
        json!(["codex", "resume", "sess-1"])
    );

    // Prompt → working; permission → blocked + permission_required.
    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "UserPromptSubmit", "pane_id": pane}),
    )
    .await
    .unwrap();
    let r = c
        .call(
            "hook-event",
            json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane}),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "blocked");
    assert_eq!(r["attention"], "permission_required");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert!(p["pane"]["last_message"]
        .as_str()
        .unwrap()
        .contains("approval"));

    // User replies → working again, attention cleared.
    let r = c
        .call(
            "hook-event",
            json!({"agent": "codex", "event": "UserPromptSubmit", "pane_id": pane}),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "working");
    assert_eq!(r["attention"], "none");

    // Stop → done + unread.
    let r = c
        .call("hook-event", json!({"agent": "codex", "event": "Stop", "pane_id": pane, "payload": {"session_id": "sess-1"}}))
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "done");
    assert_eq!(r["attention"], "unread");

    // Unknown agent rejected; unknown hook accepted as no-op.
    let err = c
        .call(
            "hook-event",
            json!({"agent": "nope", "event": "Stop", "pane_id": pane}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("unknown agent"), "{err}");
    assert!(c
        .call(
            "hook-event",
            json!({"agent": "codex", "event": "FutureEvent", "pane_id": pane})
        )
        .await
        .is_ok());
    srv.shutdown().await;
}

#[tokio::test]
async fn untitled_hook_message_is_the_last_message_verbatim() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "PreToolUse", "pane_id": pane,
               "message": "Bash(cargo test)"}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    // The notification still needs a title; the pane's headline doesn't.
    assert_eq!(p["pane"]["last_message"], "Bash(cargo test)");

    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "PreToolUse", "pane_id": pane,
               "title": "Tests", "message": "3 failed"}),
    )
    .await
    .unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["last_message"], "Tests: 3 failed");
    srv.shutdown().await;
}

#[tokio::test]
async fn hook_event_claude_notification_and_cursor_stop() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let r = c
        .call("hook-event", json!({
            "agent": "claude", "event": "Notification", "pane_id": pane,
            "payload": {"notification_type": "idle_prompt", "message": "shall I?", "session_id": "cs-1"}
        }))
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "blocked");
    assert_eq!(r["attention"], "input_required");
    let r = c
        .call(
            "hook-event",
            json!({
                "agent": "cursor", "event": "stop", "pane_id": pane,
                "payload": {"session_id": "chat-9"}
            }),
        )
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "done");
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(
        p["pane"]["agent"]["resume_argv"],
        json!(["cursor-agent", "--resume", "chat-9"])
    );
    srv.shutdown().await;
}

fn codex_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join("codex");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = --no-daemon ]; then echo fixture-local-runtime; shift; fi\ncase \"$1\" in\n--version) echo fixture-codex ;;\nresume) printf 'fixture-resume:%s\\n' \"$2\"; read line ;;\n*) exit 1 ;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[tokio::test]
async fn spawn_detects_agent_kind() {
    let srv = TestServer::start().await;
    let codex = codex_fixture(srv.socket.parent().unwrap());
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap();
    // Fixture basename drives the same detection as an installed agent.
    let t = c
        .call("tab.create", json!({"workspace_id": ws, "title": "t"}))
        .await
        .unwrap();
    let tab = t["tab"]["id"].as_str().unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": tab, "argv": [codex, "--version"]}),
        )
        .await
        .unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "codex");
    // Plain shell → generic.
    let t = c
        .call("tab.create", json!({"workspace_id": ws, "title": "t2"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": t["tab"]["id"], "argv": ["sh"]}),
        )
        .await
        .unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "generic");
    // Explicit hint wins over detection.
    let t = c
        .call("tab.create", json!({"workspace_id": ws, "title": "t3"}))
        .await
        .unwrap();
    let p = c
        .call("pane.spawn", json!({"workspace_id": ws, "tab_id": t["tab"]["id"], "argv": ["sh"], "agent_hint": "claude"}))
        .await
        .unwrap();
    assert_eq!(p["pane"]["agent"]["kind"], "claude");
    srv.shutdown().await;
}

#[tokio::test]
async fn report_session_builds_resume_and_pane_resume_spawns() {
    let agents = plugin_test_dir("codex-resume");
    let codex = codex_fixture(&agents);
    write_manifest(
        &agents,
        "codex.toml",
        &format!(
            "[agent]\nkind = \"codex\"\n[session]\nresume = [{}, \"resume\", \"{{session_id}}\"]\n",
            json!(codex)
        ),
    );
    let mut srv = TestServer::start_with_dirs(None, Some(&agents)).await;
    let mut c = srv.client().await;
    let custom = srv.integration_home.join("custom-codex");
    let ws = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let launch = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws["workspace"]["id"], "argv": [codex, "--version"],
        "env": {"CODEX_HOME": custom, "CODEX_API_KEY": "must-not-persist"}}),
        )
        .await
        .unwrap();
    let pane = launch["pane"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        launch["pane"]["agent"]["config_env"],
        json!({"CODEX_HOME": custom})
    );
    c.call(
        "wait",
        json!({"pane_id": pane, "until": "exited", "timeout_s": 15}),
    )
    .await
    .unwrap();
    let r = c
        .call(
            "report-session",
            json!({"pane_id": pane, "agent_session_id": "bogus-id", "agent": "codex"}),
        )
        .await
        .unwrap();
    assert_eq!(
        r["pane"]["agent"]["resume_argv"],
        json!([codex, "resume", "bogus-id"])
    );
    srv.restart().await;
    c = srv.client().await;
    assert!(
        !std::fs::read_to_string(srv.state_dir.join("snapshot.json"))
            .unwrap()
            .contains("must-not-persist")
    );
    let hook_file = custom.join("hooks.json");
    assert!(hook_file.is_file());
    std::fs::remove_file(&hook_file).unwrap();
    // Resume launches the fixture with the reported session, on a real PTY.
    let r = c
        .call("pane.resume", json!({"pane_id": pane}))
        .await
        .unwrap();
    assert_eq!(r["pane"]["live"]["state"], "live");
    assert_eq!(r["pane"]["restore_state"], "LIVE");
    assert_eq!(r["integration"]["status"], "configured");
    assert_eq!(r["integration"]["changed"], true);
    assert!(hook_file.is_file());
    assert_eq!(r["pane"]["title"], "codex");
    wait_for_text(
        &mut c,
        &pane,
        "fixture-resume:bogus-id",
        Duration::from_secs(5),
    )
    .await;
    wait_for_text(
        &mut c,
        &pane,
        "fixture-local-runtime",
        Duration::from_secs(5),
    )
    .await;
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["live"]["state"], "live");
    c.call("pane.close", json!({"pane_id": pane}))
        .await
        .unwrap();
    // Resuming a live pane is rejected.
    let (_ws2, pane2) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let err = c
        .call("pane.resume", json!({"pane_id": pane2}))
        .await
        .unwrap_err();
    assert!(err.contains("already live"), "{err}");
    srv.shutdown().await;
    std::fs::remove_dir_all(agents).unwrap();
}

#[tokio::test]
async fn baseline_wait_ignores_old_done_and_accepts_fast_new_work() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "hook-event",
        json!({"agent":"codex", "event":"Stop", "pane_id":pane}),
    )
    .await
    .unwrap();
    let old = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
    let baseline = old["wait_baseline"].clone();
    if !baseline.is_object() {
        srv.shutdown().await;
        panic!("pane.get must expose a new-work baseline");
    }
    let stale = c
        .call(
            "wait",
            json!({"pane_id":pane, "until":"done", "after":baseline, "timeout_s":0}),
        )
        .await;
    assert!(stale.unwrap_err().starts_with("TIMEOUT"));
    c.call(
        "hook-event",
        json!({"agent":"codex", "event":"UserPromptSubmit", "pane_id":pane}),
    )
    .await
    .unwrap();
    c.call(
        "hook-event",
        json!({"agent":"codex", "event":"Stop", "pane_id":pane}),
    )
    .await
    .unwrap();
    let result = c.call("wait", json!({"pane_id":pane, "until":["done", "blocked", "failed"], "after":baseline, "timeout_s":1})).await.unwrap();
    assert_eq!(result["outcome"], "done");
    assert!(
        result["transition_seq"].as_u64().unwrap() > baseline["lifecycle_seq"].as_u64().unwrap()
    );
    let current = c
        .call(
            "wait",
            json!({"pane_id":pane, "until":"done", "timeout_s":0}),
        )
        .await
        .unwrap();
    assert_eq!(current["satisfied"], true);
    let output = tokio::process::Command::new(signaltty_testkit::bin_path("signaltty"))
        .arg("--socket")
        .arg(&srv.socket)
        .args(["--json", "wait", "--pane"])
        .arg(&pane)
        .args(["--until", "done,blocked,failed", "--after-baseline"])
        .arg(baseline.to_string())
        .args(["--timeout", "0"])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["outcome"],
        "done"
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn baseline_wait_remembers_brief_matching_transitions() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "hook-event",
        json!({"pane_id":pane,"agent":"codex","event":"Stop"}),
    )
    .await
    .unwrap();
    let baseline =
        c.call("pane.get", json!({"pane_id":pane})).await.unwrap()["wait_baseline"].clone();
    for event in ["UserPromptSubmit", "PermissionRequest", "UserPromptSubmit"] {
        c.call(
            "hook-event",
            json!({"pane_id":pane,"agent":"codex","event":event}),
        )
        .await
        .unwrap();
    }
    let result = c
        .call(
            "wait",
            json!({"pane_id":pane,"until":["done","blocked"],"after":baseline,"timeout_s":0}),
        )
        .await
        .unwrap();
    assert_eq!(result["outcome"], "blocked");
    assert_eq!(
        result["lifecycle"], "working",
        "current state is still reported separately"
    );
    assert!(c
        .call(
            "wait",
            json!({"pane_id":pane,"until":"blocked","timeout_s":0})
        )
        .await
        .unwrap_err()
        .starts_with("TIMEOUT"));
    srv.shutdown().await;
}

#[tokio::test]
async fn baseline_wait_tracks_session_discovery_replacement_and_restart() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let initial =
        c.call("pane.get", json!({"pane_id":pane})).await.unwrap()["wait_baseline"].clone();
    c.call(
        "report-session",
        json!({"pane_id":pane,"agent":"codex","agent_session_id":"first"}),
    )
    .await
    .unwrap();
    c.call(
        "hook-event",
        json!({"pane_id":pane,"agent":"codex","event":"PermissionRequest"}),
    )
    .await
    .unwrap();
    let blocked = c.call("wait",json!({"pane_id":pane,"until":["done","blocked","failed"],"after":initial,"timeout_s":1})).await.unwrap();
    assert_eq!(blocked["outcome"], "blocked");
    let known = c.call("pane.get", json!({"pane_id":pane})).await.unwrap()["wait_baseline"].clone();
    for session in ["second", "first"] {
        c.call(
            "report-session",
            json!({"pane_id":pane,"agent":"codex","agent_session_id":session}),
        )
        .await
        .unwrap();
    }
    let replaced = c
        .call(
            "wait",
            json!({"pane_id":pane,"until":"blocked","after":known,"timeout_s":0}),
        )
        .await
        .unwrap_err();
    assert!(replaced.starts_with("IDENTITY_CHANGED"), "{replaced}");
    for params in [
        json!({"pane_id":pane,"until":[],"after":initial}),
        json!({"pane_id":pane,"until":"done","after":true}),
        json!({"pane_id":pane,"until":["done",7]}),
    ] {
        assert!(c
            .call("wait", params)
            .await
            .unwrap_err()
            .starts_with("BAD_PARAMS"));
    }
    let current =
        c.call("pane.get", json!({"pane_id":pane})).await.unwrap()["wait_baseline"].clone();
    srv.restart().await;
    let mut c = srv.client().await;
    let stale = c
        .call(
            "wait",
            json!({"pane_id":pane,"until":"exited","after":current,"timeout_s":0}),
        )
        .await
        .unwrap_err();
    assert!(stale.starts_with("IDENTITY_CHANGED"), "{stale}");
    srv.shutdown().await;
}

#[tokio::test]
async fn subscribe_declares_corrupt_history_and_cursor_ahead() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call("notify", json!({"pane_id":pane,"title":"before"}))
        .await
        .unwrap();
    let mut ahead = srv.client().await;
    let result = ahead
        .call("subscribe", json!({"from_seq":u64::MAX}))
        .await
        .unwrap();
    assert_eq!(result["subscribed"], false);
    assert_eq!(result["replay"]["status"], "cursor_ahead");
    srv.restart().await;
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(srv.state_dir.join("audit.jsonl"))
        .unwrap()
        .write_all(b"{broken\n")
        .unwrap();
    let mut sub = srv.client().await;
    let result = sub.call("subscribe", json!({"from_seq":0})).await.unwrap();
    assert_eq!(result["subscribed"], false);
    assert_eq!(result["replay"]["status"], "unavailable");
    assert_eq!(result["replay"]["returned"], 0);
    assert_eq!(result["replay"]["recovery"], "snapshot_then_resubscribe");
    assert!(
        sub.call("server.status", json!({})).await.is_err(),
        "incomplete streams close after acknowledgment"
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn wait_until_blocked_via_hook() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let pane2 = pane.clone();
    let sock = srv.socket.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut c = TestClient::connect(&sock).await.unwrap();
        c.call(
            "hook-event",
            json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pane2}),
        )
        .await
        .unwrap();
    });
    let w = c
        .call(
            "wait",
            json!({"pane_id": pane, "until": "blocked", "timeout_s": 10}),
        )
        .await
        .unwrap();
    assert_eq!(w["satisfied"], true);
    assert_eq!(w["lifecycle"], "blocked");
    srv.shutdown().await;
}

#[tokio::test]
async fn wait_timeout_and_seen() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let err = c
        .call(
            "wait",
            json!({"pane_id": pane, "until": "exited", "timeout_s": 1}),
        )
        .await
        .unwrap_err();
    assert!(err.starts_with("TIMEOUT"), "{err}");
    // Attention wait: notify in background, wait for unread.
    let pane2 = pane.clone();
    let sock = srv.socket.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let mut c = TestClient::connect(&sock).await.unwrap();
        c.call("notify", json!({"pane_id": pane2, "title": "hi"}))
            .await
            .unwrap();
    });
    let w = c
        .call(
            "wait",
            json!({"pane_id": pane, "until": "unread", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(w["satisfied"], true);
    srv.shutdown().await;
}

// ---- Phase 4: executable plugins ----

fn plugin_test_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-plugtest-{}-{}-{}",
        std::process::id(),
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_recorder_plugin(dir: &std::path::Path, log: &std::path::Path) {
    let sub = dir.join("recorder");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(
        sub.join("plugin.toml"),
        format!(
            "[plugin]\nname = \"recorder\"\nversion = \"0.1.0\"\n\n\
             [[hook]]\nevents = [\"notification.created\", \"attention.*\"]\n\
             command = [\"./record.sh\"]\n\n\
             [hook.env]\nHOOK_LOG = \"{}\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::write(
        sub.join("record.sh"),
        "#!/bin/sh\ncat >> \"$HOOK_LOG\" <<EOF\n$SIGNALTTY_EVENT :: $(cat)\nEOF\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            sub.join("record.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
}

#[tokio::test]
async fn plugin_hook_fires_on_attention() {
    let base = plugin_test_dir("hook");
    let log = base.join("events.log");
    write_recorder_plugin(&base.join("plugins"), &log);
    let srv = TestServer::start_with_plugin_dir(Some(&base.join("plugins"))).await;
    let mut c = srv.client().await;

    let list = c.call("plugin.list", json!({})).await.unwrap();
    assert_eq!(list["plugins"].as_array().unwrap().len(), 1);
    assert_eq!(list["plugins"][0]["name"], "recorder");

    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call("notify", json!({"pane_id": pane, "title": "plug-check"}))
        .await
        .unwrap();

    let start = std::time::Instant::now();
    loop {
        let body = std::fs::read_to_string(&log).unwrap_or_default();
        if body.contains("notification.created") && body.contains("attention.created") {
            // Envelope on stdin: pane id of the triggering pane.
            assert!(body.contains(&pane), "hook stdin must carry payload");
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "hook did not fire"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Stats land just after hook exit; poll briefly.
    let start = std::time::Instant::now();
    loop {
        let list = c.call("plugin.list", json!({})).await.unwrap();
        let hook = &list["plugins"][0]["hooks"][0];
        if hook["runs"].as_u64().unwrap_or(0) >= 2 {
            assert_eq!(hook["errors"], 0, "{hook}");
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "stats lag: {hook}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    srv.shutdown().await;
    std::fs::remove_dir_all(&base).ok();
}

#[tokio::test]
async fn plugin_reload_and_failure_isolation() {
    let base = plugin_test_dir("reload");
    let plugdir = base.join("plugins");
    write_recorder_plugin(&plugdir, &base.join("events.log"));
    let srv = TestServer::start_with_plugin_dir(Some(&plugdir)).await;
    let mut c = srv.client().await;

    let list = c.call("plugin.list", json!({})).await.unwrap();
    assert_eq!(list["plugins"].as_array().unwrap().len(), 1);
    assert!(list["failures"].as_array().unwrap().is_empty());

    // Add a second good plugin and one broken manifest, then reload.
    let sub = plugdir.join("second");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("plugin.toml"), "[plugin]\nname = \"second\"\n").unwrap();
    let broken = plugdir.join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("plugin.toml"), "this is [[[ not toml\n").unwrap();

    let list = c.call("plugin.reload", json!({})).await.unwrap();
    let names: Vec<String> = list["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names, vec!["recorder".to_string(), "second".to_string()]);
    let failures = list["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["dir"], "broken");
    srv.shutdown().await;
    std::fs::remove_dir_all(&base).ok();
}

fn ratio_at(layout: &serde_json::Value, path: &[usize]) -> f64 {
    let mut node = layout;
    for side in path {
        node = &node[if *side == 0 { "first" } else { "second" }];
    }
    node["ratio"].as_f64().unwrap()
}

#[tokio::test]
async fn set_ratio_persists_divider_and_clamps() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let s = c
        .call(
            "pane.split",
            json!({"pane_id": pane, "direction": "right", "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    let pane2 = s["pane"]["id"].as_str().unwrap().to_string();
    let pane2_tab = s["pane"]["tab_id"].as_str().unwrap().to_string();
    c.call(
        "pane.split",
        json!({"pane_id": pane2, "direction": "right", "argv": ["sleep", "30"]}),
    )
    .await
    .unwrap();
    let layout = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap()["tabs"][0]["layout"]
        .clone();
    // Equal thirds through the whole stack, not 1/2 + 1/4 + 1/4.
    assert!((ratio_at(&layout, &[]) - 1.0 / 3.0).abs() < 1e-6);
    assert!((ratio_at(&layout, &[1]) - 0.5).abs() < 1e-6);

    // A dragged divider persists.
    let r = c
        .call(
            "tab.set_ratio",
            json!({"tab_id": pane2_tab, "path": [], "ratio": 0.25}),
        )
        .await
        .unwrap();
    assert!((ratio_at(&r["tab"]["layout"], &[]) - 0.25).abs() < 1e-6);
    let g = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert!((ratio_at(&g["tabs"][0]["layout"], &[]) - 0.25).abs() < 1e-6);

    // Out-of-range ratios clamp instead of collapsing a pane.
    c.call(
        "tab.set_ratio",
        json!({"tab_id": pane2_tab, "path": [], "ratio": 0.99}),
    )
    .await
    .unwrap();
    let g = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert!((ratio_at(&g["tabs"][0]["layout"], &[]) - 0.95).abs() < 1e-6);

    // Bad paths and unknown tabs fail without touching the layout.
    assert!(c
        .call(
            "tab.set_ratio",
            json!({"tab_id": pane2_tab, "path": [1, 1, 1], "ratio": 0.5}),
        )
        .await
        .is_err());
    assert!(c
        .call(
            "tab.set_ratio",
            json!({"tab_id": pane2_tab, "path": [2], "ratio": 0.5}),
        )
        .await
        .is_err());
    assert!(c
        .call(
            "tab.set_ratio",
            json!({"tab_id": "tab_nope", "path": [], "ratio": 0.5}),
        )
        .await
        .is_err());

    // The divider survives a server restart.
    srv.restart().await;
    let mut c = srv.client().await;
    let g = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert!((ratio_at(&g["tabs"][0]["layout"], &[]) - 0.95).abs() < 1e-6);
    srv.shutdown().await;
}

#[tokio::test]
async fn schema_lists_every_dispatched_method() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let schema = c.call("server.schema", json!({})).await.unwrap();
    assert_eq!(schema["protocol"], "signaltty/1");
    assert!(!schema["version"].as_str().unwrap_or("").is_empty());
    let methods: Vec<String> = serde_json::from_value(schema["methods"].clone()).unwrap();
    for must in [
        "server.status",
        "server.schema",
        "pane.spawn",
        "hook-event",
        "wait",
        "focus.next_unread",
    ] {
        assert!(methods.contains(&must.to_string()), "{must} listed");
    }
    assert!(!schema["events"].as_array().unwrap().is_empty());
    assert!(!schema["codes"].as_array().unwrap().is_empty());
    // Every listed method dispatches: probing with {} may fail param
    // decode or id lookup, but never falls through to UNKNOWN_METHOD.
    for m in &methods {
        if m == "server.shutdown" {
            continue; // would stop the fixture; covered by shutdown tests
        }
        match c.call(m, json!({})).await {
            Ok(_) => {}
            Err(e) => assert!(
                !e.starts_with("UNKNOWN_METHOD"),
                "{m} listed but not dispatched: {e}"
            ),
        }
    }
    srv.shutdown().await;
}

#[tokio::test]
async fn spawn_rejects_a_tab_in_another_workspace() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let a = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let b = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let tab = c
        .call("tab.create", json!({"workspace_id": b["workspace"]["id"]}))
        .await
        .unwrap();
    let err = c.call("pane.spawn", json!({"workspace_id": a["workspace"]["id"], "tab_id": tab["tab"]["id"], "argv": ["sleep", "30"]})).await.unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    assert_eq!(
        c.call("server.status", json!({})).await.unwrap()["live_panes"],
        0
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn failed_spawn_does_not_create_an_automatic_tab() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap()["workspace"]["id"]
        .clone();
    let before = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    let err = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "argv": ["/nonexistent/signaltty-test-command"]}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("SPAWN_FAILED"), "{err}");
    let after = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(
        c.call("server.status", json!({})).await.unwrap()["live_panes"],
        0
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn layout_replacement_cannot_hide_owned_panes() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    c.call(
        "pane.split",
        json!({"pane_id": pane, "direction": "right", "argv": ["sleep", "30"]}),
    )
    .await
    .unwrap();
    let before = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap();
    let tab = before["tabs"][0]["id"].clone();
    let err = c
        .call(
            "tab.set_layout",
            json!({"tab_id": tab, "layout": {"type": "pane", "pane_id": pane}}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    assert_eq!(
        c.call("workspace.get", json!({"workspace_id": ws}))
            .await
            .unwrap()["tabs"],
        before["tabs"]
    );
    c.call("workspace.close", json!({"workspace_id": ws}))
        .await
        .unwrap();
    assert_eq!(
        c.call("server.status", json!({})).await.unwrap()["live_panes"],
        0
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn layout_rejects_duplicate_panes_and_clamps_ratios() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let tab = c
        .call("workspace.get", json!({"workspace_id": ws}))
        .await
        .unwrap()["tabs"][0]["id"]
        .clone();
    let duplicate = json!({"type": "split", "dir": "right", "ratio": 0.5, "first": {"type": "pane", "pane_id": pane}, "second": {"type": "pane", "pane_id": pane}});
    let err = c
        .call(
            "tab.set_layout",
            json!({"tab_id": tab, "layout": duplicate}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
    let sibling = c
        .call(
            "pane.split",
            json!({"pane_id": pane, "direction": "right", "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .clone();
    let layout = json!({"type": "split", "dir": "right", "ratio": 3.0, "first": {"type": "pane", "pane_id": pane}, "second": {"type": "pane", "pane_id": sibling}});
    let result = c
        .call("tab.set_layout", json!({"tab_id": tab, "layout": layout}))
        .await
        .unwrap();
    assert!((result["tab"]["layout"]["ratio"].as_f64().unwrap() - 0.95).abs() < 1e-6);
    srv.shutdown().await;
}

#[tokio::test]
async fn detach_stops_streaming_without_stopping_the_process() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let mut control = srv.client().await;
    let (_ws, pane) = new_pane(
        &mut control,
        vec!["sh", "-c", "while read line; do echo RESULT:$line; done"],
    )
    .await;
    c.call("pane.attach", json!({"pane_id": pane, "mark_seen": false}))
        .await
        .unwrap();
    c.call("pane.detach", json!({"pane_id": pane}))
        .await
        .unwrap();
    control
        .call(
            "pane.input",
            json!({"pane_id": pane, "data_b64": base64_encode("detached-marker\n")}),
        )
        .await
        .unwrap();
    wait_for_text(
        &mut control,
        &pane,
        "RESULT:detached-marker",
        Duration::from_secs(5),
    )
    .await;
    let line = c.raw_roundtrip(&json!({"protocol": "signaltty/1", "id": "after-detach", "method": "pane.get", "params": {"pane_id": pane}}).to_string()).await;
    let response: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(
        response["id"], "after-detach",
        "unexpected stream after detach: {line}"
    );
    assert_eq!(response["result"]["pane"]["live"]["state"], "live");
    srv.shutdown().await;
}

#[tokio::test]
async fn attach_snapshot_and_stream_share_output_offsets() {
    let srv = TestServer::start().await;
    let mut control = srv.client().await;
    let mut attached = srv.client().await;
    let (_ws, pane) = new_pane(
        &mut control,
        vec![
            "sh",
            "-c",
            "echo before-snapshot; while read line; do echo RESULT:$line; done",
        ],
    )
    .await;
    wait_for_text(
        &mut control,
        &pane,
        "before-snapshot",
        Duration::from_secs(5),
    )
    .await;
    let snapshot = attached
        .call("pane.attach", json!({"pane_id": pane, "mark_seen": false}))
        .await
        .unwrap();
    let offset = snapshot["output_offset"]
        .as_u64()
        .expect("snapshot must identify covered output");
    assert!(offset > 0);
    control
        .call(
            "pane.input",
            json!({"pane_id": pane, "data_b64": base64_encode("offset-marker\n")}),
        )
        .await
        .unwrap();
    let events = attached.read_events(1, Duration::from_secs(5)).await;
    assert_eq!(events[0]["event"], "pty.data");
    assert!(events[0]["payload"]["output_offset"].as_u64().unwrap() > offset);
    wait_for_text(
        &mut control,
        &pane,
        "RESULT:offset-marker",
        Duration::from_secs(5),
    )
    .await;
    let next = control
        .call("pane.attach", json!({"pane_id": pane, "mark_seen": false}))
        .await
        .unwrap();
    assert!(
        next["output_offset"].as_u64().unwrap()
            >= events[0]["payload"]["output_offset"].as_u64().unwrap()
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn concurrent_spawn_and_close_do_not_leave_a_live_pane() {
    let srv = TestServer::start().await;
    let mut spawning = srv.client().await;
    let mut closing = srv.client().await;
    for _ in 0..12 {
        let ws = spawning
            .call("workspace.create", json!({"cwd": "/tmp"}))
            .await
            .unwrap()["workspace"]["id"]
            .clone();
        let (spawned, closed) = tokio::join!(
            spawning.call(
                "pane.spawn",
                json!({"workspace_id": ws, "argv": ["sleep", "30"]})
            ),
            closing.call("workspace.close", json!({"workspace_id": ws})),
        );
        closed.unwrap();
        if let Err(error) = spawned {
            assert!(error.contains("NO_SUCH_WORKSPACE"), "{error}");
        }
        assert_eq!(
            closing.call("server.status", json!({})).await.unwrap()["live_panes"],
            0
        );
    }
    srv.shutdown().await;
}

#[tokio::test]
async fn closing_restored_legacy_layout_cleans_hidden_owned_panes() {
    for close_workspace in [false, true] {
        let mut srv = TestServer::start().await;
        let mut c = srv.client().await;
        let (ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
        let tab = c
            .call("workspace.get", json!({"workspace_id": ws}))
            .await
            .unwrap()["tabs"][0]["id"]
            .clone();
        c.call("server.shutdown", json!({"force": true}))
            .await
            .unwrap();
        drop(c);
        tokio::time::sleep(Duration::from_millis(250)).await;
        let path = srv.state_dir.join("snapshot.json");
        let mut snapshot: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        snapshot["tabs"][0]["layout"] = serde_json::Value::Null;
        std::fs::write(&path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        srv.restart().await;
        let mut c = srv.client().await;
        assert!(c.call("pane.get", json!({"pane_id": pane})).await.is_ok());
        if close_workspace {
            c.call("workspace.close", json!({"workspace_id": ws}))
                .await
                .unwrap();
        } else {
            c.call("tab.close", json!({"tab_id": tab})).await.unwrap();
        }
        let error = c
            .call("pane.get", json!({"pane_id": pane}))
            .await
            .unwrap_err();
        assert!(error.contains("NO_SUCH_PANE"), "{error}");
        srv.shutdown().await;
    }
}

#[tokio::test]
async fn attached_pane_streams_after_exit_and_resume_without_reattaching() {
    let agents = plugin_test_dir("resume-stream");
    write_manifest(
        &agents,
        "resume.toml",
        r#"
[agent]
kind = "codex"
binaries = ["sh"]
[session]
resume = ["sh", "-c", "echo resumed-stream; read line"]
"#,
    );
    let srv = TestServer::start_with_dirs(None, Some(&agents)).await;
    let mut control = srv.client().await;
    let mut attached = srv.client().await;
    let (_ws, pane) = new_pane(
        &mut control,
        vec!["sh", "-c", "read line; echo initial-stream"],
    )
    .await;
    control
        .call(
            "report-session",
            json!({"pane_id": pane, "agent": "codex", "agent_session_id": "fixture"}),
        )
        .await
        .unwrap();
    attached
        .call("pane.attach", json!({"pane_id": pane, "mark_seen": false}))
        .await
        .unwrap();
    control
        .call(
            "pane.input",
            json!({"pane_id": pane, "data_b64": base64_encode("exit\n")}),
        )
        .await
        .unwrap();
    control
        .call(
            "wait",
            json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
        )
        .await
        .unwrap();
    // Consume already queued output while confirming the initial child exited.
    let exited = attached
        .call("pane.get", json!({"pane_id": pane}))
        .await
        .unwrap();
    assert_eq!(exited["pane"]["live"]["state"], "exited");
    control
        .call("pane.resume", json!({"pane_id": pane}))
        .await
        .unwrap();
    let stale = control
        .call(
            "wait",
            json!({"pane_id":pane,"until":"unknown","after":exited["wait_baseline"],"timeout_s":0}),
        )
        .await
        .unwrap_err();
    assert!(stale.starts_with("IDENTITY_CHANGED"), "{stale}");
    let streamed = attached.read_events(1, Duration::from_secs(3)).await;
    assert_eq!(streamed[0]["event"], "pty.data");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(streamed[0]["payload"]["data_b64"].as_str().unwrap())
        .unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("resumed-stream"));
    srv.shutdown().await;
    std::fs::remove_dir_all(agents).ok();
}

#[tokio::test]
async fn simultaneous_resume_requests_launch_only_one_child() {
    let agents = plugin_test_dir("resume-race");
    write_manifest(
        &agents,
        "resume.toml",
        r#"
[agent]
kind = "codex"
binaries = ["sh"]
[session]
resume = ["sh", "-c", "read line"]
"#,
    );
    let srv = TestServer::start_with_dirs(None, Some(&agents)).await;
    let mut a = srv.client().await;
    let mut b = srv.client().await;
    for _ in 0..8 {
        let (_ws, pane) = new_pane(&mut a, vec!["sh", "-c", "exit 0"]).await;
        a.call(
            "wait",
            json!({"pane_id": pane, "until": "exited", "timeout_s": 5}),
        )
        .await
        .unwrap();
        a.call(
            "report-session",
            json!({"pane_id": pane, "agent": "codex", "agent_session_id": "fixture"}),
        )
        .await
        .unwrap();
        let (first, second) = tokio::join!(
            a.call("pane.resume", json!({"pane_id": pane})),
            b.call("pane.resume", json!({"pane_id": pane})),
        );
        assert_ne!(
            first.is_ok(),
            second.is_ok(),
            "exactly one resume must succeed: {first:?}, {second:?}"
        );
        let error = first.err().or_else(|| second.err()).unwrap();
        assert!(error.contains("already live"), "{error}");
        a.call("pane.close", json!({"pane_id": pane}))
            .await
            .unwrap();
    }
    srv.shutdown().await;
    std::fs::remove_dir_all(agents).ok();
}

fn lifecycle_fixture(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, r#"#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
root = pathlib.Path(os.environ.get('CLAUDE_CONFIG_DIR', str(pathlib.Path(os.environ['SIGNALTTY_INTEGRATION_HOME']) / '.claude')))
print('agent-started', flush=True)
try:
    hooks = json.loads((root / 'settings.json').read_text())['hooks']
except Exception:
    hooks = {}
print('hooks-loaded' if hooks else 'hooks-unavailable', flush=True)
for line in sys.stdin:
    event = line.strip()
    for group in hooks.get(event, []):
        for hook in group['hooks']:
            subprocess.run(hook['command'], shell=True, input=json.dumps({'session_id':'fixture', 'notification_type':'permission_prompt'}).encode(), check=True)
    print('event-delivered:' + event, flush=True)
"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[tokio::test]
async fn automatic_hooks_are_loaded_before_spawn_and_split_and_drive_states() {
    use base64::Engine;
    let srv = TestServer::start().await;
    let claude = lifecycle_fixture(srv.socket.parent().unwrap(), "claude");
    let file = srv.integration_home.join(".claude/settings.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        r#"{"model":"opus","hooks":{"Stop":[{"hooks":[{"type":"command","command":"true"}]}]}}"#,
    )
    .unwrap();
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let result = c
        .call(
            "pane.spawn",
            json!({"workspace_id":w["workspace"]["id"], "argv":[claude]}),
        )
        .await
        .unwrap();
    assert_eq!(result["integration"]["status"], "configured");
    assert_eq!(result["integration"]["changed"], true);
    let pane = result["pane"]["id"].as_str().unwrap();
    wait_for_text(&mut c, pane, "hooks-loaded", Duration::from_secs(5)).await;
    for (event, state) in [
        ("SessionStart", "idle"),
        ("UserPromptSubmit", "working"),
        ("Notification", "blocked"),
        ("Stop", "done"),
    ] {
        c.call("pane.input", json!({"pane_id":pane,"data_b64":base64::engine::general_purpose::STANDARD.encode(format!("{event}\n"))})).await.unwrap();
        wait_for_text(
            &mut c,
            pane,
            &format!("event-delivered:{event}"),
            Duration::from_secs(5),
        )
        .await;
        let p = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
        assert_eq!(p["pane"]["lifecycle"], state);
    }
    let root: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(root["model"], "opus");
    assert_eq!(root["hooks"]["Stop"][0]["hooks"][0]["command"], "true");
    std::fs::remove_file(&file).unwrap();
    let split = c
        .call("pane.split", json!({"pane_id":pane,"argv":[claude]}))
        .await
        .unwrap();
    assert_eq!(split["integration"]["changed"], true);
    assert_eq!(split["pane"]["agent"]["kind"], "claude");
    wait_for_text(
        &mut c,
        split["pane"]["id"].as_str().unwrap(),
        "hooks-loaded",
        Duration::from_secs(5),
    )
    .await;
    srv.shutdown().await;
}

#[tokio::test]
async fn automatic_setup_failure_is_visible_without_stopping_the_agent() {
    let srv = TestServer::start().await;
    let claude = lifecycle_fixture(srv.socket.parent().unwrap(), "claude");
    let file = srv.integration_home.join(".claude/settings.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{broken").unwrap();
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let r = c
        .call(
            "pane.spawn",
            json!({"workspace_id":w["workspace"]["id"],"argv":[claude]}),
        )
        .await
        .unwrap();
    assert_eq!(r["integration"]["status"], "error");
    assert!(r["integration"]["notice"]
        .as_str()
        .unwrap()
        .contains("invalid JSON"));
    wait_for_text(
        &mut c,
        r["pane"]["id"].as_str().unwrap(),
        "agent-started",
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(std::fs::read_to_string(file).unwrap(), "{broken");
    srv.shutdown().await;
}

#[tokio::test]
async fn hints_shells_and_ambiguous_aliases_do_not_authorize_setup() {
    let srv = TestServer::start().await;
    let agent = lifecycle_fixture(srv.socket.parent().unwrap(), "agent");
    let mut c = srv.client().await;
    for argv in [json!(["sh", "-c", "read line"]), json!([agent])] {
        let w = c
            .call("workspace.create", json!({"cwd":"/tmp"}))
            .await
            .unwrap();
        let r = c
            .call(
                "pane.spawn",
                json!({"workspace_id":w["workspace"]["id"],"argv":argv,"agent_hint":"cursor"}),
            )
            .await
            .unwrap();
        assert!(r.get("integration").is_none());
    }
    assert!(!srv.integration_home.join(".cursor/hooks.json").exists());
    srv.shutdown().await;
}

#[tokio::test]
async fn explicit_launch_config_root_and_disabled_mode_are_respected() {
    let srv = TestServer::start().await;
    let claude = lifecycle_fixture(srv.socket.parent().unwrap(), "claude");
    let custom = srv.integration_home.join("custom-claude");
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let r=c.call("pane.spawn",json!({"workspace_id":w["workspace"]["id"],"argv":[claude],"env":{"CLAUDE_CONFIG_DIR":custom}})).await.unwrap();
    assert_eq!(
        r["integration"]["file"],
        custom.join("settings.json").to_string_lossy().as_ref()
    );
    wait_for_text(
        &mut c,
        r["pane"]["id"].as_str().unwrap(),
        "hooks-loaded",
        Duration::from_secs(5),
    )
    .await;
    assert!(!srv.integration_home.join(".claude/settings.json").exists());
    let split = c
        .call(
            "pane.split",
            json!({"pane_id":r["pane"]["id"],"argv":[claude,"--bare"]}),
        )
        .await
        .unwrap();
    assert_eq!(split["integration"]["status"], "disabled");
    let alternate = c
        .call(
            "pane.split",
            json!({"pane_id":r["pane"]["id"],"argv":[claude,"--setting-sources=project,local"]}),
        )
        .await
        .unwrap();
    assert_eq!(alternate["integration"]["status"], "disabled");
    assert!(!srv.integration_home.join(".claude/settings.json").exists());
    srv.shutdown().await;
}

#[tokio::test]
async fn codex_local_runtime_routes_installed_hooks_to_each_pane() {
    use std::os::unix::fs::PermissionsExt;
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (ws, stale) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let codex = srv.socket.parent().unwrap().join("codex");
    std::fs::write(&codex, r#"#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
if sys.argv[1:] == ['--no-daemon', '--version']:
    print('codex 0.159.1'); sys.exit(0)
hooks = json.loads((pathlib.Path(os.environ['SIGNALTTY_INTEGRATION_HOME']) / '.codex/hooks.json').read_text())['hooks']
env = os.environ.copy()
if '--no-daemon' not in sys.argv[1:]:
    env['SIGNALTTY_PANE'] = os.environ['CODEX_TEST_STALE_PANE']
print('runtime-ready', flush=True)
for line in sys.stdin:
    event = line.strip()
    for group in hooks.get(event, []):
        for hook in group['hooks']:
            subprocess.run(hook['command'], shell=True, input=b'{"session_id":"fixture-local"}', env=env, check=True)
    print('delivered:' + event, flush=True)
"#).unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
    let tab = c
        .call("tab.create", json!({"workspace_id":ws}))
        .await
        .unwrap();
    let first = c
        .call(
            "pane.spawn",
            json!({"workspace_id":ws,"tab_id":tab["tab"]["id"],"argv":[codex],
        "env":{"CODEX_TEST_STALE_PANE":stale}}),
        )
        .await
        .unwrap();
    let first = first["pane"]["id"].as_str().unwrap().to_owned();
    let second = c
        .call(
            "pane.split",
            json!({"pane_id":first,"direction":"right","argv":[codex],
        "env":{"CODEX_TEST_STALE_PANE":stale}}),
        )
        .await
        .unwrap();
    let second = second["pane"]["id"].as_str().unwrap().to_owned();
    let hooks = std::fs::read(srv.integration_home.join(".codex/hooks.json")).unwrap();
    for (pane, event, state) in [
        (&first, "UserPromptSubmit", "working"),
        (&second, "Stop", "done"),
    ] {
        wait_for_text(&mut c, pane, "runtime-ready", Duration::from_secs(5)).await;
        c.call(
            "pane.input",
            json!({"pane_id":pane,"data_b64":base64_encode(&format!("{event}\n"))}),
        )
        .await
        .unwrap();
        wait_for_text(
            &mut c,
            pane,
            &format!("delivered:{event}"),
            Duration::from_secs(5),
        )
        .await;
        assert_eq!(
            c.call("pane.get", json!({"pane_id":pane})).await.unwrap()["pane"]["lifecycle"],
            state
        );
    }
    assert_eq!(
        c.call("pane.get", json!({"pane_id":first})).await.unwrap()["pane"]["lifecycle"],
        "working"
    );
    assert_eq!(
        c.call("pane.get", json!({"pane_id":stale})).await.unwrap()["pane"]["lifecycle"],
        "unknown"
    );
    assert_eq!(
        hooks,
        std::fs::read(srv.integration_home.join(".codex/hooks.json")).unwrap()
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn codex_unsupported_or_hanging_probe_keeps_original_launch_with_notice() {
    use std::os::unix::fs::PermissionsExt;
    for (probe, bad_config) in [
        ("exit 2", false),
        ("sleep 30 & echo $! > probe-child.pid; wait", false),
        ("exit 2", true),
    ] {
        let srv = TestServer::start().await;
        let codex = srv.socket.parent().unwrap().join("codex");
        if bad_config {
            let config = srv.integration_home.join(".codex/hooks.json");
            std::fs::create_dir_all(config.parent().unwrap()).unwrap();
            std::fs::write(config, "broken-config").unwrap();
        }
        std::fs::write(&codex, format!("#!/bin/sh\nif [ \"$1\" = --no-daemon ] && [ \"$2\" = --version ]; then {probe}; fi\nprintf 'fallback-ready:%s\\n' \"$*\"\nread line\n")).unwrap();
        std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut c = srv.client().await;
        let ws = c
            .call(
                "workspace.create",
                json!({"cwd":srv.socket.parent().unwrap()}),
            )
            .await
            .unwrap();
        let started = std::time::Instant::now();
        let launch = c
            .call(
                "pane.spawn",
                json!({"workspace_id":ws["workspace"]["id"],"argv":[codex,"--model","exec"]}),
            )
            .await
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(
            launch["integration"]["status"],
            if bad_config { "error" } else { "disabled" }
        );
        assert!(launch["integration"]["notice"]
            .as_str()
            .unwrap()
            .contains("--no-daemon"));
        let pane = launch["pane"]["id"].as_str().unwrap();
        wait_for_text(
            &mut c,
            pane,
            "fallback-ready:--model exec",
            Duration::from_secs(3),
        )
        .await;
        assert_eq!(launch["pane"]["argv"], json!([codex, "--model", "exec"]));
        let pid_file = srv.socket.parent().unwrap().join("probe-child.pid");
        let child_stopped = if pid_file.exists() {
            let pid = std::fs::read_to_string(&pid_file).unwrap();
            let path = format!("/proc/{}/stat", pid.trim());
            tokio::time::sleep(Duration::from_millis(50)).await;
            std::fs::read_to_string(path)
                .map(|stat| stat.split_whitespace().nth(2) == Some("Z"))
                .unwrap_or(true)
        } else {
            true
        };
        if !child_stopped {
            let pid: i32 = std::fs::read_to_string(&pid_file)
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        srv.shutdown().await;
        assert!(
            child_stopped,
            "timed-out version wrapper left its child running"
        );
    }
}

#[tokio::test]
async fn request_split_across_writes_survives_interleaved_events() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let srv = TestServer::start().await;
    let mut other = srv.client().await;
    let (r, mut w) = tokio::net::UnixStream::connect(&srv.socket)
        .await
        .unwrap()
        .into_split();
    let mut reader = BufReader::new(r);
    let req = r#"{"protocol":"signaltty/1","id":"split","method":"server.status","params":{}}"#;
    let (head, tail) = req.split_at(req.len() / 2);
    w.write_all(head.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Any broadcast wakes the connection's event branch mid-line.
    other
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    w.write_all(format!("{tail}\n").as_bytes()).await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(5), reader.read_line(&mut line))
        .await
        .expect("response timeout")
        .unwrap();
    let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(resp["id"], "split", "{resp}");
    assert_eq!(resp["ok"], true, "{resp}");
    srv.shutdown().await;
}

fn processes_with_arg(arg: &str) -> usize {
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(|e| std::fs::read(e.ok()?.path().join("cmdline")).ok())
        .filter(|cmd| cmd.split(|&b| b == 0).any(|a| a == arg.as_bytes()))
        .count()
}

#[tokio::test]
async fn close_with_unknown_signal_is_rejected_and_keeps_child() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let arg = format!(
        "{}.{}",
        90000 + std::process::id() % 9000,
        std::process::id()
    );
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", &arg]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(processes_with_arg(&arg), 1);
    let err = c
        .call("pane.close", json!({"pane_id": pane, "signal": "BOGUS"}))
        .await
        .unwrap_err();
    assert!(err.starts_with("BAD_PARAMS"), "{err}");
    let err = c
        .call("pane.signal", json!({"pane_id": pane, "signal": "BOGUS"}))
        .await
        .unwrap_err();
    assert!(err.starts_with("BAD_PARAMS"), "{err}");
    c.call("pane.close", json!({"pane_id": pane}))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(processes_with_arg(&arg), 0, "child outlived its pane");
    srv.shutdown().await;
}

#[tokio::test]
async fn attach_to_missing_pane_emits_no_resize() {
    let srv = TestServer::start().await;
    let mut events = srv.client().await;
    let mut c = srv.client().await;
    events
        .call("subscribe", json!({"events": ["*"]}))
        .await
        .unwrap();
    let err = c
        .call(
            "pane.attach",
            json!({"pane_id": "pane_missing", "cols": 100, "rows": 30}),
        )
        .await
        .unwrap_err();
    assert!(err.starts_with("NO_SUCH_PANE"), "{err}");
    c.call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ev = events.read_events(1, Duration::from_secs(5)).await;
    assert_eq!(ev[0]["event"], "workspace.created", "{}", ev[0]);
    srv.shutdown().await;
}

#[tokio::test]
async fn concurrent_workspace_creates_get_distinct_handles() {
    let srv = TestServer::start().await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let mut c = srv.client().await;
        tasks.spawn(async move {
            c.call("workspace.create", json!({"cwd": "/tmp", "name": "api"}))
                .await
                .unwrap()["workspace"]["handle"]
                .as_str()
                .unwrap()
                .to_string()
        });
    }
    let mut handles = tasks.join_all().await;
    handles.sort();
    handles.dedup();
    assert_eq!(handles.len(), 8, "{handles:?}");
    srv.shutdown().await;
}

#[tokio::test]
async fn tool_hooks_start_work_but_never_clear_blocked() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let (_ws, pane) = new_pane(&mut c, vec!["sleep", "30"]).await;
    let hook = |event: &str| json!({"agent": "claude", "event": event, "pane_id": pane});

    c.call("hook-event", hook("SessionStart")).await.unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_ne!(p["pane"]["lifecycle"], "working");
    c.call("hook-event", hook("PreToolUse")).await.unwrap();
    let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
    assert_eq!(p["pane"]["lifecycle"], "working", "idle pane starts work");

    c.call("hook-event", hook("PermissionRequest"))
        .await
        .unwrap();
    for event in ["PreToolUse", "PostToolUse", "PreCompact", "PostCompact"] {
        c.call("hook-event", hook(event)).await.unwrap();
        let p = c.call("pane.get", json!({"pane_id": pane})).await.unwrap();
        assert_eq!(p["pane"]["lifecycle"], "blocked", "{event} kept blocked");
        assert_eq!(p["pane"]["attention"], "permission_required");
    }
    srv.shutdown().await;
}

/// A server whose test never calls `shutdown` (panic, early return) must
/// still be reaped when it drops.
#[tokio::test]
async fn dropped_test_server_is_reaped() {
    let srv = TestServer::start().await;
    let socket = srv.socket.to_string_lossy().to_string();
    let running = || {
        std::fs::read_dir("/proc").unwrap().flatten().any(|e| {
            std::fs::read(e.path().join("cmdline"))
                .is_ok_and(|c| String::from_utf8_lossy(&c).contains(&socket))
        })
    };
    assert!(running(), "server should be running");
    drop(srv);
    assert!(!running(), "dropped server is still running");
}
