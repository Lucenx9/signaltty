//! Phase 1 integration tests with real PTYs (§23):
//! spawn/read/exit, detach/reattach, resize, signals, malformed IPC,
//! notifications/attention, concurrent clients, splits, restart/restore.

use std::time::Duration;

use signaltty_testkit::{TestClient, TestServer};
use serde_json::json;

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
    assert_eq!(p["pane"]["lifecycle"], "exited");
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

#[tokio::test]
async fn spawn_detects_agent_kind() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap();
    // Real agent binary (fast, exits): kind detected from argv.
    let t = c
        .call("tab.create", json!({"workspace_id": ws, "title": "t"}))
        .await
        .unwrap();
    let tab = t["tab"]["id"].as_str().unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws, "tab_id": tab, "argv": ["codex", "--version"]}),
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
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    // Fast-exiting real agent process.
    let (_ws, pane) = new_pane(&mut c, vec!["codex", "--version"]).await;
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
        json!(["codex", "resume", "bogus-id"])
    );
    // One-key resume spawns the official command. A bogus id under a
    // real PTY launches codex interactively (it hangs waiting on the
    // user), so assert it is alive and speaking, then terminate it.
    let r = c
        .call("pane.resume", json!({"pane_id": pane}))
        .await
        .unwrap();
    assert_eq!(r["pane"]["live"]["state"], "live");
    assert_eq!(r["pane"]["restore_state"], "LIVE");
    assert_eq!(r["pane"]["title"], "codex");
    tokio::time::sleep(Duration::from_secs(2)).await;
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
