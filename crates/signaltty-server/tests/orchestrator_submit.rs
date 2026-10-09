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
async fn submit_retries_enter_once_when_no_activity_follows() {
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

    // Raw tty + `cat -v` prints every CR the pane receives as `^M`.
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["sh", "-c", "stty raw -echo; exec cat -v"]}),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap().to_string();

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

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
    assert_eq!(resp.error.unwrap().code, code::TIMEOUT);

    let mut text = String::new();
    for _ in 0..50 {
        let r = c
            .call(
                "pane.read",
                json!({"pane_id": pane_id, "mode": "tail", "lines": 20}),
            )
            .await
            .unwrap();
        text = r["text"].as_str().unwrap_or("").to_string();
        if text.matches("^M").count() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(text.matches("^M").count(), 2, "pane saw: {text:?}");

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
    tokio::time::sleep(Duration::from_millis(200)).await;
    driver.prompt_submit(&mut c).await.unwrap();

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

#[tokio::test]
async fn submit_accepts_blocked_input_required_without_decision_and_refuses_permission() {
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

    // 1. Case A: Blocked with permission_required -> REFUSED
    let t1 = c
        .call("tab.create", json!({"workspace_id": ws_id}))
        .await
        .unwrap();
    let p1 = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": t1["tab"]["id"], "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id_1 = p1["pane"]["id"].as_str().unwrap().to_string();

    c.call(
        "hook-event",
        json!({
            "agent": "claude",
            "event": "Notification",
            "pane_id": pane_id_1,
            "payload": {
                "notification_type": "permission_prompt",
                "message": "Allow command?",
                "session_id": "sess-perm"
            }
        }),
    )
    .await
    .unwrap();

    let err_perm = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id_1,
                "text": "yes please",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err_perm.starts_with(code::AGENT_BUSY),
        "expected AGENT_BUSY for permission_required, got {err_perm}"
    );

    // 2. Case B: Blocked with pending_decision -> REFUSED
    let t2 = c
        .call("tab.create", json!({"workspace_id": ws_id}))
        .await
        .unwrap();
    let p2 = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": t2["tab"]["id"], "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id_2 = p2["pane"]["id"].as_str().unwrap().to_string();

    c.call(
        "hook-event",
        json!({
            "agent": "codex",
            "event": "PermissionRequest",
            "pane_id": pane_id_2,
            "decision": {
                "id": "dec_1",
                "prompt": "Pick an option",
                "options": [{"id": "opt1", "label": "Option 1"}]
            },
            "payload": {"session_id": "sess-dec"}
        }),
    )
    .await
    .unwrap();

    let err_dec = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id_2,
                "text": "my answer",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err_dec.starts_with(code::AGENT_BUSY),
        "expected AGENT_BUSY for pending_decision, got {err_dec}"
    );

    // 3. Case C: Blocked + attention input_required with NO pending_decision -> ACCEPTED
    let t3 = c
        .call("tab.create", json!({"workspace_id": ws_id}))
        .await
        .unwrap();
    let p3 = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": t3["tab"]["id"], "argv": ["cat"]}),
        )
        .await
        .unwrap();
    let pane_id_3 = p3["pane"]["id"].as_str().unwrap().to_string();

    c.call(
        "hook-event",
        json!({
            "agent": "claude",
            "event": "Notification",
            "pane_id": pane_id_3,
            "payload": {
                "notification_type": "idle_prompt",
                "message": "waiting for text",
                "session_id": "sess-idle-prompt"
            }
        }),
    )
    .await
    .unwrap();

    let pane_get = c
        .call("pane.get", json!({"pane_id": pane_id_3}))
        .await
        .unwrap();
    assert_eq!(pane_get["pane"]["lifecycle"], "blocked");
    assert_eq!(pane_get["pane"]["attention"], "input_required");
    assert!(pane_get["pane"]["pending_decision"].is_null());

    let sock = srv.socket.clone();
    let pid = pane_id_3.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "claude",
                        "event": "UserPromptSubmit",
                        "pane_id": pid,
                        "payload": {"session_id": "sess-idle-prompt"}
                    }),
                )
                .await;
        }
    });

    let res = c
        .call(
            "pane.submit",
            json!({
                "pane_id": pane_id_3,
                "text": "here is my text",
                "submit_delay_ms": 10,
                "stall_timeout_s": 2,
            }),
        )
        .await
        .unwrap();

    assert_eq!(res["submitted"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_rejects_embedded_bracketed_paste_markers_before_any_write() {
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

    let read_before = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_before = read_before["text"].as_str().unwrap_or_default().to_string();

    // Reject, do not strip. Either marker ends or opens paste mode early.
    for (label, text) in [
        ("end", "hello\u{1b}[201~LEAKED_END\r"),
        ("start", "hello\u{1b}[200~LEAKED_START"),
    ] {
        let err = c
            .call(
                "pane.submit",
                json!({
                    "pane_id": pane_id,
                    "text": text,
                    "submit_delay_ms": 10,
                    "stall_timeout_s": 1,
                }),
            )
            .await
            .unwrap_err();
        assert!(
            err.starts_with(code::BAD_PARAMS),
            "{label} marker must be BAD_PARAMS, got {err}"
        );
    }

    let read_after = c
        .call("pane.read", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let text_after = read_after["text"].as_str().unwrap_or_default();
    assert_eq!(
        text_before, text_after,
        "paste markers must be refused before any PTY write"
    );
    assert!(
        !text_after.contains("LEAKED_END") && !text_after.contains("LEAKED_START"),
        "refused paste leaked into the pane: {text_after:?}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn submit_does_not_send_delayed_enter_to_new_permission_request() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let input_path = srv.state_dir.join("received-input");
    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path(), "name": "submit-race"}),
        )
        .await
        .unwrap();
    let recorder = "import os, sys, tty\ntty.setraw(0)\nwith open(sys.argv[1], 'wb', buffering=0) as f:\n while True:\n  f.write(os.read(0, 4096))\n";
    let pane = c
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws["workspace"]["id"],
                "argv": ["python3", "-c", recorder, input_path]
            }),
        )
        .await
        .unwrap();
    let pane_id = pane["pane"]["id"].as_str().unwrap().to_string();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !input_path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("raw PTY recorder ready");
    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();

    let mut submit_client = srv.client().await;
    let submit_pane = pane_id.clone();
    let submit = tokio::spawn(async move {
        submit_client
            .call_raw_resp(
                "pane.submit",
                json!({
                    "pane_id": submit_pane, "text": "review changes",
                    "submit_delay_ms": 1000, "stall_timeout_s": 1
                }),
            )
            .await
    });
    let expected = b"\x1b[200~review changes\x1b[201~";
    tokio::time::timeout(Duration::from_secs(2), async {
        while std::fs::read(&input_path).unwrap() != expected {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("paste received before permission hook");
    driver
        .permission_request(&mut c, "Bash", json!({"command": "dangerous command"}))
        .await
        .unwrap();
    let outcome = submit.await.unwrap();
    // Let the raw reader consume any erroneous Enter before inspecting bytes.
    c.call(
        "pane.input",
        json!({"pane_id": pane_id, "data_b64": "UkVDT1JERVJfQkFSUklFUg=="}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !std::fs::read(&input_path)
            .unwrap()
            .ends_with(b"RECORDER_BARRIER")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("input recorder drained");
    let received = std::fs::read(&input_path).unwrap();
    assert_eq!(
        received,
        [expected.as_slice(), b"RECORDER_BARRIER"].concat(),
        "delayed Enter must never answer a newly arrived permission request"
    );
    let response = outcome.unwrap();
    let error = response
        .error
        .expect("permission appeared during paste delay");
    assert_eq!(error.code, code::AGENT_BUSY);
    assert_eq!(
        error.details,
        json!({"stage": "delayed_enter", "paste_delivered": true})
    );
    let current = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    assert_eq!(current["pane"]["lifecycle"], "blocked");
    assert_eq!(current["pane"]["attention"], "permission_required");
    srv.shutdown().await;
}

#[tokio::test]
async fn background_submit_permission_after_paste_keeps_task_recoverable() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let input_path = srv.state_dir.join("background-input");
    let recorder = "import os, sys, tty\ntty.setraw(0)\nwith open(sys.argv[1], 'wb', buffering=0) as f:\n while True:\n  f.write(os.read(0, 4096))\n";
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path(), "contract": {"objective": "review changes"},
                "argv": ["python3", "-c", recorder, input_path], "agent": "codex",
                "submit_delay_ms": 1000, "stall_timeout_s": 1
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap();
    let pane_id = start["pane"]["id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !input_path.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("raw recorder ready");
    let driver = FakeAgentPane::new(pane_id, "codex");
    driver.session_start(&mut c, repo.path()).await.unwrap();
    let paste = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let bytes = std::fs::read(&input_path).unwrap();
            if bytes.ends_with(b"\x1b[201~") {
                break bytes;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("first prompt pasted");
    driver
        .permission_request(&mut c, "Bash", json!({"command": "needs permission"}))
        .await
        .unwrap();
    let settled = c
        .call("task.wait", json!({"task_id": task_id, "timeout_s": 3}))
        .await
        .unwrap();
    assert_eq!(
        settled["tasks"][0]["state"], "input_required",
        "a live worker waiting for permission must not become terminal"
    );
    let current = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    assert_eq!(
        settled["tasks"][0]["status_reason"],
        json!({
            "reason": "decision_required", "decision_id": current["pane"]["pending_decision"]["id"]
        })
    );
    c.call(
        "pane.input",
        json!({"pane_id": pane_id, "data_b64": "UkVDT1JERVJfQkFSUklFUg=="}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !std::fs::read(&input_path)
            .unwrap()
            .ends_with(b"RECORDER_BARRIER")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("recorder drained");
    assert_eq!(
        std::fs::read(&input_path).unwrap(),
        [paste, b"RECORDER_BARRIER".to_vec()].concat()
    );
    // An eventual result can still be accepted; a failed task rejects reports.
    let report = c
        .call(
            "task.report",
            json!({"task_id": task_id,
        "status": "completed", "summary": "finished after resolving permission"}),
        )
        .await
        .unwrap();
    assert_eq!(report["task"]["state"], "completed");
    srv.shutdown().await;
}
