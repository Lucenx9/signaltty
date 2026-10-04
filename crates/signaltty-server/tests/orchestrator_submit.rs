use std::time::Duration;

use serde_json::json;
use signaltty_proto::code;
use signaltty_testkit::{FakeAgentPane, TempGitRepo, TestServer};

#[tokio::test]
async fn submit_accept_on_idle_and_done() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call("pane.spawn", json!({"workspace_id": ws_id, "argv": ["sh"]}))
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

    // 1. Submit on idle -> transitions to working
    let sock = srv.socket.clone();
    let pid = pane_id.clone();
    let sess = driver.session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid,
                        "payload": {"session_id": sess}
                    }),
                )
                .await;
        }
    });

    let res = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "echo hello",
                "submit_delay_ms": 10,
                "stall_timeout_s": 2,
            }),
        )
        .await
        .unwrap();

    assert_eq!(res["submitted"], true);
    assert_eq!(res["outcome"], "working");

    // 2. Drive to done -> submit on done
    driver.stop(&mut c).await.unwrap();

    let sock = srv.socket.clone();
    let pid = pane_id.clone();
    let sess = driver.session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid,
                        "payload": {"session_id": sess}
                    }),
                )
                .await;
        }
    });

    let res_done = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "echo next",
                "submit_delay_ms": 10,
                "stall_timeout_s": 2,
            }),
        )
        .await
        .unwrap();

    assert_eq!(res_done["submitted"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_agent_busy_on_working_and_blocked_with_zero_bytes_written() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();
    driver.prompt_submit(&mut c).await.unwrap();

    // Now in working state
    let read_before = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_before = read_before["text"].as_str().unwrap_or_default();

    let err = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "SHOULD_NOT_BE_WRITTEN_1",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::AGENT_BUSY),
        "expected AGENT_BUSY, got {err}"
    );

    let read_after = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_after = read_after["text"].as_str().unwrap_or_default();
    assert_eq!(
        text_before, text_after,
        "zero bytes must be written on refusal"
    );

    // Drive to blocked state (PermissionRequest)
    driver
        .permission_request(&mut c, "Bash", json!({"cmd": "ls"}))
        .await
        .unwrap();

    let read_blocked_before = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_blocked_before = read_blocked_before["text"].as_str().unwrap_or_default();

    let err_blocked = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "SHOULD_NOT_BE_WRITTEN_2",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err_blocked.starts_with(code::AGENT_BUSY),
        "expected AGENT_BUSY, got {err_blocked}"
    );

    let read_blocked_after = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_blocked_after = read_blocked_after["text"].as_str().unwrap_or_default();
    assert_eq!(
        text_blocked_before, text_blocked_after,
        "zero bytes must be written on refusal"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_agent_not_ready_on_unknown_and_never_started() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call("pane.spawn", json!({"workspace_id": ws_id, "argv": ["sh"]}))
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    // Never-started pane has unknown lifecycle
    let err = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "hello",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::AGENT_NOT_READY),
        "expected AGENT_NOT_READY, got {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_agent_not_ready_on_worker_pane_with_pending_task() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Start a task which spawns a worker pane in pending state
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "pending objective"},
                "label": "worker-pending-test",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let pane_id = start["task"]["pane_id"].as_str().unwrap();
    assert_eq!(start["task"]["state"], "pending");

    // An external client calling pane.submit on this worker pane must receive AGENT_NOT_READY
    let err = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "interrupting prompt",
            }),
        )
        .await
        .unwrap_err();

    assert!(
        err.starts_with(code::AGENT_NOT_READY),
        "expected AGENT_NOT_READY, got {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_pane_exited_on_exited_pane() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    // Spawn a process that exits immediately
    let p = c
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws_id,
                "argv": ["sh", "-c", "exit 0"]
            }),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let err = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "hello",
            }),
        )
        .await
        .unwrap_err();

    assert!(
        err.starts_with(code::PANE_EXITED),
        "expected PANE_EXITED, got {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_activity_gate_timeout() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

    // Do NOT transition to working or blocked. Wait for activity gate to time out.
    let resp = c
        .call_raw_resp(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "hello stall",
                "submit_delay_ms": 10,
                "stall_timeout_s": 1,
            }),
        )
        .await
        .unwrap();

    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, code::TIMEOUT);
    assert_eq!(err.details["stage"], "activity_gate");

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_fast_completion_match() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

    // Spawn concurrent submit and fast transition
    let sock = srv.socket.clone();
    let pid = pane_id.clone();
    let sess = driver.session_id.clone();
    tokio::spawn(async move {
        // Very fast hook right after submit is initiated
        tokio::time::sleep(Duration::from_millis(15)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid,
                        "payload": {"session_id": sess}
                    }),
                )
                .await;
        }
    });

    let res = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "fast prompt",
                "submit_delay_ms": 10,
                "stall_timeout_s": 2,
            }),
        )
        .await
        .unwrap();

    assert_eq!(res["submitted"], true);
    assert_eq!(res["outcome"], "working");

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_follow_up_on_input_required_task_resumes_working() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "input required follow up test"},
                "label": "worker-input-req",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let pane_id = start["task"]["pane_id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

    // Wait until working
    let wait = c
        .call(
            "task.wait",
            json!({"task_id": task_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    // Stop without reporting -> task moves to input_required
    driver.stop(&mut c).await.unwrap();

    let task_info = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_info["task"]["state"], "input_required");

    // Send follow-up prompt via pane.submit
    let sock = srv.socket.clone();
    let pid = pane_id.clone();
    let sess = driver.session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid,
                        "payload": {"session_id": sess}
                    }),
                )
                .await;
        }
    });

    let res = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id,
                "text": "please complete the task",
                "submit_delay_ms": 10,
                "stall_timeout_s": 2,
            }),
        )
        .await
        .unwrap();
    assert_eq!(res["submitted"], true);

    // Verify task state is resumed to working!
    let task_resumed = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_resumed["task"]["state"], "working");

    srv.shutdown().await;
}
