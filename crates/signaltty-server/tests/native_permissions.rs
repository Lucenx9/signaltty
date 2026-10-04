use std::time::Duration;

use serde_json::{json, Value};
use signaltty_testkit::{TestClient, TestServer};

async fn pending(c: &mut TestClient, pane: &str) -> Value {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let result = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
        if let Some(decision) = result["pane"].get("pending_decision") {
            return decision.clone();
        }
        assert!(
            tokio::time::Instant::now() < until,
            "no native decision: {result}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn text(c: &mut TestClient, pane: &str, marker: &str) -> String {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let result = c
            .call(
                "pane.read",
                json!({"pane_id":pane,"mode":"tail","lines":100}),
            )
            .await
            .unwrap();
        let text = result["text"].as_str().unwrap_or_default();
        if text.contains(marker) {
            return text.to_owned();
        }
        assert!(tokio::time::Instant::now() < until, "no {marker}: {text}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn provider_fixture(srv: &TestServer, agent: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = srv.socket.parent().unwrap().join(agent);
    std::fs::write(&path, r#"#!/usr/bin/env python3
import json, os, pathlib, subprocess, sys
agent = pathlib.Path(sys.argv[0]).name
root = pathlib.Path(os.environ['SIGNALTTY_INTEGRATION_HOME'])
file = root / ('.claude/settings.json' if agent == 'claude' else '.codex/hooks.json')
hooks = json.loads(file.read_text())['hooks']
payload = {'session_id':'native-session', 'hook_event_name':'PermissionRequest', 'tool_name':'Bash', 'tool_input':{'command':'printf native-proof'}}
for group in hooks['PermissionRequest']:
    for hook in group['hooks']:
        result = subprocess.run(hook['command'], shell=True, input=json.dumps(payload), text=True, capture_output=True)
        print('native-output:' + result.stdout.strip(), flush=True)
print('native-finished', flush=True)
for line in sys.stdin:
    print('terminal-input:' + line.strip(), flush=True)
"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}

#[tokio::test]
async fn installed_reporters_return_once_and_deny_without_typing_terminal_input() {
    for (agent, option, behavior) in [("codex", "once", "allow"), ("claude", "deny", "deny")] {
        let srv = TestServer::start().await;
        let provider = provider_fixture(&srv, agent);
        let mut c = srv.client().await;
        let ws = c
            .call("workspace.create", json!({"cwd":"/tmp"}))
            .await
            .unwrap();
        let spawned = c
            .call(
                "pane.spawn",
                json!({"workspace_id":ws["workspace"]["id"],"argv":[provider],"cols":240}),
            )
            .await
            .unwrap();
        let pane = spawned["pane"]["id"].as_str().unwrap();
        let decision = pending(&mut c, pane).await;
        assert_eq!(decision["answerable"], true);
        let file = srv.integration_home.join(if agent == "claude" {
            ".claude/settings.json"
        } else {
            ".codex/hooks.json"
        });
        let hooks: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
        let installed = &hooks["hooks"]["PermissionRequest"][0]["hooks"][0];
        assert_eq!(installed["timeout"], 125);
        assert!(installed["command"]
            .as_str()
            .unwrap()
            .contains("--wait-for-answer"));
        assert!(decision["prompt"]
            .as_str()
            .unwrap()
            .contains("printf native-proof"));
        assert_eq!(
            decision["options"],
            json!([{"id":"once","label":"Allow once"},{"id":"deny","label":"Deny"}])
        );
        c.call("pane.mark_seen", json!({"pane_id":pane}))
            .await
            .unwrap();
        let answered = c
            .call(
                "decision.answer",
                json!({"pane_id":pane,"decision_id":decision["id"],"option_id":option}),
            )
            .await
            .unwrap();
        assert_eq!(answered["answered"], true);
        let output = text(&mut c, pane, "native-finished").await;
        let raw = output
            .lines()
            .find_map(|line| line.strip_prefix("native-output:"))
            .unwrap();
        let verdict: Value = serde_json::from_str(raw.trim()).unwrap();
        assert_eq!(
            verdict["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(
            verdict["hookSpecificOutput"]["decision"]["behavior"],
            behavior
        );
        assert!(!output.contains("terminal-input:"), "{output}");
        let stale = c
            .call(
                "decision.answer",
                json!({"pane_id":pane,"decision_id":decision["id"],"option_id":option}),
            )
            .await
            .unwrap_err();
        assert!(stale.starts_with("NO_SUCH_DECISION"), "{stale}");
        srv.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_or_unrelated_native_wait_does_not_mutate_pane() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let ws = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id":ws["workspace"]["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    let pane = p["pane"]["id"].as_str().unwrap();
    for params in [
        json!({"agent":"codex","event":"Stop","pane_id":pane,"wait_for_answer":true}),
        json!({"agent":"claude","event":"PermissionRequest","pane_id":pane,"wait_for_answer":true,"payload":{"session_id":"s","tool_name":42}}),
        json!({"agent":"codex","event":"PermissionRequest","pane_id":pane,"wait_for_answer":true,"wait_timeout_s":0,"payload":{"session_id":"s","tool_name":"Bash","tool_input":{}}}),
    ] {
        let error = c.call("hook-event", params).await.unwrap_err();
        assert!(error.starts_with("BAD_PARAMS"), "{error}");
        let after = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
        assert_eq!(after["pane"]["lifecycle"], p["pane"]["lifecycle"]);
        assert!(after["pane"].get("pending_decision").is_none());
    }
    srv.shutdown().await;
}

async fn waiting_request(
    srv: &TestServer,
    pane: &str,
    timeout: u64,
) -> tokio::task::JoinHandle<Result<Value, String>> {
    let mut hook = srv.client().await;
    let pane = pane.to_owned();
    tokio::spawn(async move {
        hook.call("hook-event", json!({"agent":"codex","event":"PermissionRequest","pane_id":pane,
            "wait_for_answer":true,"wait_timeout_s":timeout,
            "payload":{"session_id":"native-session","tool_name":"Bash","tool_input":{"command":"true"}}})).await
    })
}

async fn live_pane(c: &mut TestClient) -> String {
    let ws = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let result = c
        .call(
            "pane.spawn",
            json!({"workspace_id":ws["workspace"]["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    result["pane"]["id"].as_str().unwrap().to_owned()
}

async fn no_decision(c: &mut TestClient, pane: &str) {
    let until = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        let result = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
        if result["pane"].get("pending_decision").is_none() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "stale decision: {result}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn disconnect_timeout_supersede_and_session_loss_never_grant() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let pane = live_pane(&mut c).await;
    let first = waiting_request(&srv, &pane, 10).await;
    let first_id = pending(&mut c, &pane).await["id"].clone();
    first.abort();
    let _ = first.await;
    no_decision(&mut c, &pane).await;
    assert!(c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":first_id,"option_id":"once"})
        )
        .await
        .unwrap_err()
        .starts_with("NO_SUCH_DECISION"));
    let timeout = waiting_request(&srv, &pane, 1).await;
    pending(&mut c, &pane).await;
    let result = timeout.await.unwrap().unwrap();
    assert!(result["native_verdict"].is_null());
    assert_eq!(result["cancelled"], "timeout");
    no_decision(&mut c, &pane).await;
    let old = waiting_request(&srv, &pane, 10).await;
    let old_id = pending(&mut c, &pane).await["id"].clone();
    let new = waiting_request(&srv, &pane, 10).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(3);
    let new_id = loop {
        let decision = pending(&mut c, &pane).await;
        if decision["id"] != old_id {
            break decision["id"].clone();
        }
        assert!(tokio::time::Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(old.await.unwrap().unwrap()["native_verdict"].is_null());
    assert!(c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":old_id,"option_id":"once"})
        )
        .await
        .unwrap_err()
        .starts_with("NO_SUCH_DECISION"));
    c.call(
        "report-session",
        json!({"pane_id":pane,"agent":"codex","agent_session_id":"replacement"}),
    )
    .await
    .unwrap();
    assert!(new.await.unwrap().unwrap()["native_verdict"].is_null());
    no_decision(&mut c, &pane).await;
    assert!(c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":new_id,"option_id":"once"})
        )
        .await
        .unwrap_err()
        .starts_with("NO_SUCH_DECISION"));
    let exit = waiting_request(&srv, &pane, 10).await;
    pending(&mut c, &pane).await;
    c.call("pane.signal", json!({"pane_id":pane,"signal":"TERM"}))
        .await
        .unwrap();
    assert!(exit.await.unwrap().unwrap()["native_verdict"].is_null());
    no_decision(&mut c, &pane).await;
    srv.shutdown().await;
}

#[tokio::test]
async fn unconnected_native_permission_is_read_only() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let pane = live_pane(&mut c).await;
    c.call("hook-event", json!({"agent":"codex","event":"PermissionRequest","pane_id":pane,
        "payload":{"session_id":"native-session","tool_name":"Bash","tool_input":{"command":"true"}}})).await.unwrap();
    let decision = pending(&mut c, &pane).await;
    assert_eq!(decision["answerable"], false);
    let error = c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":decision["id"],"option_id":"once"}),
        )
        .await
        .unwrap_err();
    assert!(error.starts_with("NO_SUCH_DECISION"), "{error}");
    srv.shutdown().await;
}

#[tokio::test]
async fn identity_change_and_concurrent_picks_cannot_deliver_old_grants() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let pane = live_pane(&mut c).await;
    for _ in 0..12 {
        let hook = waiting_request(&srv, &pane, 10).await;
        let id = pending(&mut c, &pane).await["id"].clone();
        c.call(
            "report-session",
            json!({"pane_id":pane,"agent":"codex","agent_session_id":"new-session"}),
        )
        .await
        .unwrap();
        let error = c
            .call(
                "decision.answer",
                json!({"pane_id":pane,"decision_id":id,"option_id":"once"}),
            )
            .await
            .unwrap_err();
        assert!(error.starts_with("NO_SUCH_DECISION"), "{error}");
        assert!(hook.await.unwrap().unwrap()["native_verdict"].is_null());
    }
    let hook = waiting_request(&srv, &pane, 10).await;
    let id = pending(&mut c, &pane).await["id"].clone();
    let mut a = srv.client().await;
    let mut b = srv.client().await;
    let params = json!({"pane_id":pane,"decision_id":id,"option_id":"once"});
    let (a, b) = tokio::join!(
        a.call("decision.answer", params.clone()),
        b.call("decision.answer", params)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(
        hook.await.unwrap().unwrap()["native_verdict"]["hookSpecificOutput"]["decision"]
            ["behavior"],
        "allow"
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn shutdown_and_restart_cannot_restore_an_answer_channel() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;
    let pane = live_pane(&mut c).await;
    let hook = waiting_request(&srv, &pane, 10).await;
    let id = pending(&mut c, &pane).await["id"].clone();
    srv.restart().await;
    let result = hook.await.unwrap();
    assert!(result.is_err() || result.unwrap()["native_verdict"].is_null());
    let mut c = srv.client().await;
    let pane_result = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
    assert_eq!(pane_result["pane"]["pending_decision"]["answerable"], false);
    assert!(c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":id,"option_id":"once"})
        )
        .await
        .is_err());
    srv.shutdown().await;
}

#[tokio::test]
async fn replayed_render_data_cannot_claim_a_live_native_route() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let pane = live_pane(&mut c).await;
    let hook = waiting_request(&srv, &pane, 10).await;
    let decision = pending(&mut c, &pane).await;
    c.call(
        "hook-event",
        json!({"agent":"codex","event":"PermissionRequest","pane_id":pane,
        "payload":{"session_id":"native-session"},"decision":decision}),
    )
    .await
    .unwrap();
    let current = c.call("pane.get", json!({"pane_id":pane})).await.unwrap();
    if let Some(decision) = current["pane"].get("pending_decision") {
        assert_eq!(decision["answerable"], false);
    }
    assert!(c
        .call(
            "decision.answer",
            json!({"pane_id":pane,"decision_id":decision["id"],"option_id":"once"})
        )
        .await
        .unwrap_err()
        .starts_with("NO_SUCH_DECISION"));
    assert!(hook.await.unwrap().unwrap()["native_verdict"].is_null());
    srv.shutdown().await;
}
