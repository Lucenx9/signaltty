use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use signaltty_core::model::{
    Contract, Disposition, DispositionOutcome, Relationship, Task, TaskResult, TaskResultStatus,
};
use signaltty_core::state::TaskState;
use signaltty_testkit::{FakeAgentPane, TempGitRepo, TestServer};

#[tokio::test]
async fn legacy_snapshot_without_tasks_or_lineage_loads_cleanly() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Create a workspace and tab
    let ws = c
        .call(
            "workspace.create",
            json!({"name": "legacy-ws", "cwd": "/tmp"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let tab = c
        .call("tab.create", json!({"workspace_id": ws_id, "title": "t1"}))
        .await
        .unwrap();
    let tab_id = tab["tab"]["id"].as_str().unwrap();

    // Spawn a pane
    let pane = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": tab_id, "argv": ["sh"]}),
        )
        .await
        .unwrap();
    let pane_id = pane["pane"]["id"].as_str().unwrap();

    // Gracefully shutdown to write snapshot
    c.call("server.shutdown", json!({"force": true}))
        .await
        .unwrap();
    drop(c);
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Rewrite snapshot to simulate a legacy pre-018 snapshot:
    // Remove "tasks" field completely, and remove lineage fields from panes
    let snap_path = srv.state_dir.join("snapshot.json");
    let mut snap: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snap_path).unwrap()).unwrap();

    let obj = snap.as_object_mut().unwrap();
    obj.remove("tasks"); // legacy snapshot had no tasks array

    if let Some(panes) = obj.get_mut("panes").and_then(|p| p.as_array_mut()) {
        for p in panes {
            let pane_obj = p.as_object_mut().unwrap();
            pane_obj.remove("parent_pane_id");
            pane_obj.remove("root_pane_id");
            pane_obj.remove("label");
            pane_obj.remove("relationship");
            pane_obj.remove("task_id");
        }
    }

    std::fs::write(&snap_path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();

    // Restart server on the legacy snapshot
    srv.restart().await;
    let mut c = srv.client().await;

    // Pane should load cleanly
    let pane_res = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    let restored_pane = &pane_res["pane"];
    assert_eq!(restored_pane["id"], pane_id);
    assert_eq!(restored_pane["parent_pane_id"], serde_json::Value::Null);
    assert_eq!(restored_pane["root_pane_id"], serde_json::Value::Null);
    assert_eq!(restored_pane["label"], serde_json::Value::Null);
    assert_eq!(restored_pane["relationship"], serde_json::Value::Null);
    assert_eq!(restored_pane["task_id"], serde_json::Value::Null);

    srv.shutdown().await;
}

#[tokio::test]
async fn snapshot_roundtrips_orchestrator_tasks_and_lineage() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"name": "task-ws", "cwd": "/tmp"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let tab = c
        .call("tab.create", json!({"workspace_id": ws_id, "title": "t1"}))
        .await
        .unwrap();
    let tab_id = tab["tab"]["id"].as_str().unwrap();

    let pane = c
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws_id,
                "tab_id": tab_id,
                "argv": ["sh"],
                "parent_pane_id": "parent_p1",
                "label": "subagent-1",
                "relationship": "subagent"
            }),
        )
        .await
        .unwrap();
    let pane_id = pane["pane"]["id"].as_str().unwrap();

    // Create a task directly in store to test snapshot serialization/deserialization
    let task_id = signaltty_core::ids::new_task_id();
    let context_id = signaltty_core::ids::new_context_id();
    let now = Utc::now();
    let task = Task {
        id: task_id.clone(),
        context_id: context_id.clone(),
        parent_task_id: None,
        pane_id: Some(pane_id.to_string()),
        parent_pane_id: Some("parent_p1".to_string()),
        root_pane_id: Some("root_p0".to_string()),
        relationship: Relationship::Subagent,
        label: "roundtrip-task".to_string(),
        contract: Contract::new("Test objective").unwrap(),
        agent: Some("codex".to_string()),
        source_repo: PathBuf::from("/tmp/repo"),
        target_branch: Some("feature-1".to_string()),
        worktree_path: PathBuf::from("/tmp/wt"),
        branch: "task-branch".to_string(),
        preexisting_branch: true,
        base_ref: "feature-1".to_string(),
        base_sha: "abcdef0123456789".to_string(),
        state: TaskState::Completed,
        result: Some(TaskResult {
            status: TaskResultStatus::Completed,
            summary: "Done nicely".to_string(),
            artifacts: vec![],
            evidence: None,
            reported_at: now,
        }),
        disposition: Disposition {
            outcome: DispositionOutcome::Merged,
            target_ref: Some("feature-1".to_string()),
            merged_sha: Some("sha123".to_string()),
            branch_deleted: Some(false),
            at: Some(now),
        },
        status_reason: None,
        finish_error: None,
        worker_pid: Some(1234),
        worker_cmd: Some(vec!["sh".into()]),
        created_at: now,
        updated_at: now,
    };

    // Insert task into server snapshot manually or inject via store
    c.call("server.shutdown", json!({"force": true}))
        .await
        .unwrap();
    drop(c);
    tokio::time::sleep(Duration::from_millis(250)).await;

    let snap_path = srv.state_dir.join("snapshot.json");
    let mut snap: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&snap_path).unwrap()).unwrap();

    // Put task into snapshot.json tasks list
    let task_val = serde_json::to_value(&task).unwrap();
    snap["tasks"] = json!([task_val]);
    std::fs::write(&snap_path, serde_json::to_vec_pretty(&snap).unwrap()).unwrap();

    // Restart server and verify snapshot loads with task
    srv.restart().await;

    // Read back snapshot directly after restart
    let mut cfg = signaltty_server::config::Config::from_env();
    cfg.state_dir = srv.state_dir.clone();
    let loaded = signaltty_server::persist::load(&cfg).unwrap();

    assert_eq!(loaded.snapshot.tasks.len(), 1);
    let loaded_task = &loaded.snapshot.tasks[0];
    assert_eq!(loaded_task.id, task_id);
    assert_eq!(loaded_task.context_id, context_id);
    assert_eq!(loaded_task.state, TaskState::Completed);
    assert_eq!(loaded_task.target_branch, Some("feature-1".to_string()));
    assert!(loaded_task.preexisting_branch);
    assert_eq!(loaded_task.disposition.outcome, DispositionOutcome::Merged);

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_async_happy_path() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"name": "ws", "cwd": repo.path().to_string_lossy()}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let orch = c
        .call("pane.spawn", json!({"workspace_id": ws_id, "argv": ["sh"]}))
        .await
        .unwrap();
    let orch_pane_id = orch["pane"]["id"].as_str().unwrap().to_string();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {
                    "objective": "Build happy path feature",
                    "acceptance_criteria": ["feature done"]
                },
                "label": "worker-happy",
                "parent_pane_id": orch_pane_id,
                "context_id": "ctx_happy",
                "agent": "codex",
                "argv": ["sh"]
            }),
        )
        .await
        .unwrap();

    let task = &start_res["task"];
    let pane = &start_res["pane"];
    assert_eq!(task["state"], "pending");
    assert_eq!(task["context_id"], "ctx_happy");
    assert_eq!(task["label"], "worker-happy");
    assert_eq!(task["parent_pane_id"], orch_pane_id);
    assert_eq!(task["target_branch"], "main");
    assert_eq!(task["preexisting_branch"], false);
    assert_eq!(pane["id"], task["pane_id"]);
    assert_eq!(pane["parent_pane_id"], orch_pane_id);
    assert_eq!(pane["label"], "worker-happy");

    let worker_pane_id = pane["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(task["worktree_path"].as_str().unwrap());
    assert!(wt_path.exists());

    // Fake agent drives worker to idle, then background submit moves task to working
    let driver = FakeAgentPane::new(&worker_pane_id, "codex");
    driver.session_start(&mut c, &wt_path).await.unwrap();

    let wait = c
        .call(
            "task.wait",
            json!({"task_id": task["id"], "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    let get = c
        .call("task.get", json!({"task_id": task["id"]}))
        .await
        .unwrap();
    assert_eq!(get["task"]["state"], "working");

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_pane_closed_before_ready_fails_task_and_server_remains_responsive() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {
                    "objective": "Task to exit early",
                },
                "label": "worker-exit",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let worker_pane_id = start_res["pane"]["id"].as_str().unwrap().to_string();

    // Close the pane before it becomes ready
    c.call("pane.close", json!({"pane_id": worker_pane_id}))
        .await
        .unwrap();

    // Give the background task a moment to detect and update
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The server must NOT deadlock: it must answer subsequent calls
    let get = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(get["task"]["state"], "failed");
    assert!(!get["task"]["status_reason"].is_null());
    // Operator close is pane death (FR-019): the synchronous close fails the
    // task with the proximate cause; the background ready-wait then no-ops
    // on the terminal task instead of racing it with `ready_timeout`.
    assert_eq!(get["task"]["status_reason"]["reason"], "pane_closed");

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_rate_limited_over_cap() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Start 4 tasks (default cap is 4)
    for i in 0..4 {
        let res = c
            .call(
                "task.start",
                json!({
                    "repo": repo.path().to_string_lossy(),
                    "contract": {"objective": format!("Objective {i}")},
                    "label": format!("worker-{i}"),
                    "argv": ["sh"],
                }),
            )
            .await;
        assert!(res.is_ok(), "task {i} should start");
    }

    // 5th task must fail with RATE_LIMITED
    let fifth = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Fifth objective"},
                "label": "worker-5",
                "argv": ["sh"],
            }),
        )
        .await;

    let err = fifth.unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::RATE_LIMITED));

    // Assert exactly 4 tasks exist
    let list = c.call("task.list", json!({})).await.unwrap();
    assert_eq!(list["tasks"].as_array().unwrap().len(), 4);

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_empty_objective_refused() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "   "},
            }),
        )
        .await;

    let err = res.unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));

    let list = c.call("task.list", json!({})).await.unwrap();
    assert!(list["tasks"].as_array().unwrap().is_empty());

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_bad_base_ref_refused() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Valid objective"},
                "base_ref": "nonexistent-branch-or-tag-12345",
                "argv": ["sh"],
            }),
        )
        .await;

    let err = res.unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));

    let list = c.call("task.list", json!({})).await.unwrap();
    assert!(list["tasks"].as_array().unwrap().is_empty());

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_existing_branch_adopted() {
    let repo = TempGitRepo::new();
    repo.create_branch("pre-existing-feature");
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Valid objective"},
                "branch": "pre-existing-feature",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    assert_eq!(res["task"]["branch"], "pre-existing-feature");
    assert_eq!(res["task"]["preexisting_branch"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_concurrent_same_path_serialized() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let wt_target = std::env::temp_dir().join(format!(
        "signaltty-test-wt-same-path-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&wt_target);

    let start_1 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "First"},
                "branch": "branch-1",
                "path": wt_target.to_string_lossy(),
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    assert_eq!(start_1["task"]["state"], "pending");

    let start_2 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Second"},
                "branch": "branch-2",
                "path": wt_target.to_string_lossy(),
                "argv": ["sh"],
            }),
        )
        .await;

    let err = start_2.unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));

    let _ = std::fs::remove_dir_all(&wt_target);
    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_default_worktree_path_vs_explicit() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Explicit path
    let explicit_path =
        std::env::temp_dir().join(format!("signaltty-test-wt-explicit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&explicit_path);

    let res_explicit = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Explicit path"},
                "branch": "branch-explicit",
                "path": explicit_path.to_string_lossy(),
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        res_explicit["task"]["worktree_path"].as_str().unwrap(),
        explicit_path.to_string_lossy()
    );

    // Default path
    let res_default = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Default path"},
                "branch": "branch-default",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let default_path_str = res_default["task"]["worktree_path"].as_str().unwrap();
    assert!(default_path_str.contains("signaltty/worktrees/"));

    let _ = std::fs::remove_dir_all(&explicit_path);
    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_detached_head_target_branch_unset() {
    let repo = TempGitRepo::new();
    repo.git(&["checkout", "--detach", "HEAD"]);

    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Detached head test"},
                "branch": "branch-detached",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    assert!(res["task"]["target_branch"].is_null());

    srv.shutdown().await;
}

#[tokio::test]
async fn task_get_and_list_roundtrip() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let start_1 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Task 1"},
                "context_id": "ctx_A",
                "branch": "b1",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let t1_id = start_1["task"]["id"].as_str().unwrap();

    let start_2 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Task 2"},
                "context_id": "ctx_B",
                "branch": "b2",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let _t2_id = start_2["task"]["id"].as_str().unwrap();

    // task.get
    let get_1 = c.call("task.get", json!({"task_id": t1_id})).await.unwrap();
    assert_eq!(get_1["task"]["id"], t1_id);

    let get_unknown = c.call("task.get", json!({"task_id": "task_unknown"})).await;
    assert!(get_unknown
        .unwrap_err()
        .starts_with(signaltty_proto::code::NO_SUCH_TASK));

    // task.list all
    let list_all = c.call("task.list", json!({})).await.unwrap();
    assert_eq!(list_all["tasks"].as_array().unwrap().len(), 2);

    // task.list by context_id
    let list_ctx_a = c
        .call("task.list", json!({"context_id": "ctx_A"}))
        .await
        .unwrap();
    assert_eq!(list_ctx_a["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(list_ctx_a["tasks"][0]["id"], t1_id);

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_agent_and_argv_resolution() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // 1. Neither agent nor argv -> BAD_PARAMS
    let res_neither = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "No agent or argv"},
            }),
        )
        .await;
    let err = res_neither.unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));

    // 2. Agent given without argv -> spawns agent interactive default binary
    let res_agent = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Agent default"},
                "agent": "claude",
            }),
        )
        .await
        .unwrap();
    let pane = &res_agent["pane"];
    let task = &res_agent["task"];
    assert_eq!(task["worker_cmd"], json!(["claude"]));
    assert_eq!(pane["agent"]["kind"], "claude");

    // 3. Explicit argv wins when given alongside agent
    let res_argv = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Argv wins"},
                "agent": "claude",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let pane_override = &res_argv["pane"];
    let task_override = &res_argv["task"];
    assert_eq!(task_override["worker_cmd"], json!(["sh"]));
    assert_eq!(pane_override["agent"]["kind"], "claude");

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_concurrent_calls_respect_atomic_reservation() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Start 3 tasks (leaving exactly 1 slot available out of 4)
    for i in 0..3 {
        let res = c
            .call(
                "task.start",
                json!({
                    "repo": repo.path().to_string_lossy(),
                    "contract": {"objective": format!("Objective {i}")},
                    "label": format!("worker-{i}"),
                    "argv": ["sh"],
                }),
            )
            .await;
        assert!(res.is_ok());
    }

    let mut c1 = srv.client().await;
    let mut c2 = srv.client().await;
    let repo_path = repo.path().to_string_lossy().to_string();

    let fut1 = c1.call(
        "task.start",
        json!({
            "repo": repo_path.clone(),
            "contract": {"objective": "Concurrent 1"},
            "label": "worker-c1",
            "argv": ["sh"],
        }),
    );
    let fut2 = c2.call(
        "task.start",
        json!({
            "repo": repo_path.clone(),
            "contract": {"objective": "Concurrent 2"},
            "label": "worker-c2",
            "argv": ["sh"],
        }),
    );

    let (res1, res2) = tokio::join!(fut1, fut2);

    let ok_count = [res1.is_ok(), res2.is_ok()].iter().filter(|&&b| b).count();
    let rate_limited_count = [res1.as_ref().err(), res2.as_ref().err()]
        .into_iter()
        .flatten()
        .filter(|e| e.starts_with(signaltty_proto::code::RATE_LIMITED))
        .count();

    assert_eq!(
        ok_count, 1,
        "Exactly one concurrent task.start must succeed"
    );
    assert_eq!(
        rate_limited_count, 1,
        "Exactly one concurrent task.start must be RATE_LIMITED"
    );

    let list = c.call("task.list", json!({})).await.unwrap();
    assert_eq!(
        list["tasks"].as_array().unwrap().len(),
        4,
        "Total active tasks must not exceed cap of 4"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_counts_input_required_towards_cap() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Start 4 tasks
    let res0 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Objective 0"},
                "label": "worker-0",
                "argv": ["sh"],
                "agent": "codex",
            }),
        )
        .await
        .unwrap();

    for i in 1..4 {
        c.call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": format!("Objective {i}")},
                "label": format!("worker-{i}"),
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    }

    let worker_pane = res0["task"]["pane_id"].as_str().unwrap();
    let driver = FakeAgentPane::new(worker_pane, "codex");
    let wt = std::path::PathBuf::from(res0["task"]["worktree_path"].as_str().unwrap());
    driver.session_start(&mut c, &wt).await.unwrap();

    let wait = c
        .call(
            "task.wait",
            json!({"task_id": res0["task"]["id"], "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    // Stop hook event moves working task to InputRequired
    driver.stop(&mut c).await.unwrap();

    let get = c
        .call("task.get", json!({"task_id": res0["task"]["id"]}))
        .await
        .unwrap();
    assert_eq!(get["task"]["state"], "input_required");

    // Attempting a 5th task must be RATE_LIMITED because InputRequired counts against cap
    let res5 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Fifth objective"},
                "argv": ["sh"],
            }),
        )
        .await;

    let err = res5.unwrap_err();
    assert!(
        err.starts_with(signaltty_proto::code::RATE_LIMITED),
        "Expected RATE_LIMITED, got: {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_in_workspace_without_tabs_does_not_panic() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let wt = repo.path().parent().unwrap().join("st-wt-empty-tabs");

    // 1. Create directory, register workspace, then delete directory
    std::fs::create_dir_all(&wt).unwrap();
    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": wt.to_string_lossy(), "name": "ws-empty"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let ws_get = c
        .call("workspace.get", json!({"workspace_id": ws_id}))
        .await
        .unwrap();
    assert!(ws_get["workspace"]["tabs"].as_array().unwrap().is_empty());
    assert!(ws_get["workspace"]["active_tab_id"].is_null());

    std::fs::remove_dir_all(&wt).unwrap();

    // 2. task.start targeting wt
    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "path": wt.to_string_lossy(),
                "contract": {"objective": "Empty tabs test"},
                "argv": ["sh"],
            }),
        )
        .await;

    assert!(
        res.is_ok(),
        "task.start must succeed without panicking: {:?}",
        res
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_report_lifecycle_and_result() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Report test"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = res["task"]["id"].as_str().unwrap();

    let rep = c
        .call(
            "task.report",
            json!({
                "task_id": task_id,
                "status": "completed",
                "summary": "Completed work successfully",
                "artifacts": [{"name": "out", "path": "out.txt"}],
                "evidence": {"note": "verified"}
            }),
        )
        .await
        .unwrap();

    assert_eq!(rep["task"]["state"], "completed");
    assert_eq!(
        rep["task"]["result"]["summary"],
        "Completed work successfully"
    );

    // Second report refused
    let err = c
        .call(
            "task.report",
            json!({
                "task_id": task_id,
                "status": "failed",
                "summary": "Trying to report again",
            }),
        )
        .await
        .unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));

    // Unknown task refused
    let err2 = c
        .call(
            "task.report",
            json!({
                "task_id": "task_nonexistent",
                "status": "completed",
                "summary": "none",
            }),
        )
        .await
        .unwrap_err();
    assert!(err2.starts_with(signaltty_proto::code::NO_SUCH_TASK));

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_wait_settled_and_context() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Empty context wait matches immediately
    let res = c
        .call(
            "task.wait",
            json!({
                "context_id": "ctx-empty-123",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(res["satisfied"], true);
    assert_eq!(res["tasks"].as_array().unwrap().len(), 0);

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Wait test"},
                "argv": ["sh"],
                "context_id": "ctx-wait-test",
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap();

    // Report completed in background
    let task_id_clone = task_id.to_string();
    let mut c2 = srv.client().await;
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = c2
            .call(
                "task.report",
                json!({
                    "task_id": task_id_clone,
                    "status": "completed",
                    "summary": "Done wait test",
                }),
            )
            .await;
    });

    let wait_res = c
        .call(
            "task.wait",
            json!({
                "task_id": task_id,
                "until": "settled",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(wait_res["satisfied"], true);
    assert_eq!(wait_res["tasks"][0]["state"], "completed");

    srv.shutdown().await;
}

#[tokio::test]
async fn test_attention_pending_ranking() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call("workspace.create", json!({"cwd": "/tmp", "name": "ws-att"}))
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let tab1 = c
        .call("tab.create", json!({"workspace_id": ws_id, "title": "t1"}))
        .await
        .unwrap();
    let tab1_id = tab1["tab"]["id"].as_str().unwrap();

    let p1 = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": tab1_id, "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    let pid1 = p1["pane"]["id"].as_str().unwrap();

    let tab2 = c
        .call("tab.create", json!({"workspace_id": ws_id, "title": "t2"}))
        .await
        .unwrap();
    let tab2_id = tab2["tab"]["id"].as_str().unwrap();

    let p2 = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "tab_id": tab2_id, "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    let pid2 = p2["pane"]["id"].as_str().unwrap();

    // p1 gets unread, p2 gets permission_required
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "Stop", "pane_id": pid1}),
    )
    .await
    .unwrap();

    c.call(
        "hook-event",
        json!({"agent": "codex", "event": "PermissionRequest", "pane_id": pid2}),
    )
    .await
    .unwrap();

    let pending = c
        .call("attention.pending", json!({"limit": 10}))
        .await
        .unwrap();
    let panes = pending["panes"].as_array().unwrap();

    // permission_required > unread
    assert!(panes.len() >= 2);
    assert_eq!(panes[0]["pane_id"], pid2);
    assert_eq!(panes[0]["attention"], "permission_required");
    assert_eq!(panes[1]["pane_id"], pid1);
    assert_eq!(panes[1]["attention"], "unread");

    srv.shutdown().await;
}

#[tokio::test]
async fn test_subscribe_task_ids_filter_and_replay() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let t1 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "T1"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let tid1 = t1["task"]["id"].as_str().unwrap();

    let t2 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "T2"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let _tid2 = t2["task"]["id"].as_str().unwrap();

    let mut sub_client = srv.client().await;
    let sub = sub_client
        .call(
            "subscribe",
            json!({
                "events": ["task.*"],
                "task_ids": [tid1],
                "from_seq": 0,
            }),
        )
        .await
        .unwrap();
    assert_eq!(sub["subscribed"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn test_turn_ended_without_report_and_follow_up_submit_and_report() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Turn ended test"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = res["task"]["id"].as_str().unwrap().to_string();
    let pane_id = res["pane"]["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(res["task"]["worktree_path"].as_str().unwrap());

    // Fake agent drives worker to idle, then background submit moves task to working
    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt_path).await.unwrap();

    // Wait until working
    let wait = c
        .call(
            "task.wait",
            json!({
                "task_id": task_id,
                "until": "working",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    // Simulate agent ending turn (Stop) without reporting
    let _hres = c
        .call(
            "hook-event",
            json!({
                "agent": "codex",
                "event": "Stop",
                "pane_id": pane_id,
                "message": "I finished the turn without report",
            }),
        )
        .await
        .unwrap();

    // Task moves to input_required with reason turn_ended_without_report
    let task_get = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_get["task"]["state"], "input_required");
    assert_eq!(
        task_get["task"]["status_reason"]["reason"],
        "turn_ended_without_report"
    );

    // Follow-up submit resumes working
    let sock = srv.socket.clone();
    let pid_cl = pane_id.clone();
    let sess_cl = driver.session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid_cl,
                        "payload": {"session_id": sess_cl}
                    }),
                )
                .await;
        }
    });

    let sub = c
        .call(
            "pane.submit",
            json!({
                "pane_id": &pane_id,
                "text": "echo continuing",
            }),
        )
        .await
        .unwrap();
    assert_eq!(sub["submitted"], true);

    let task_get2 = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_get2["task"]["state"], "working");

    // Report completes the task
    let rep = c
        .call(
            "task.report",
            json!({
                "pane_id": &pane_id,
                "status": "completed",
                "summary": "Completed after follow up",
            }),
        )
        .await
        .unwrap();
    assert_eq!(rep["task"]["state"], "completed");

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_native_permission_block_and_answer() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Permission test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = res["task"]["id"].as_str().unwrap().to_string();
    let pane_id = res["pane"]["id"].as_str().unwrap().to_string();

    // Raise permission request hook
    c.call(
        "hook-event",
        json!({
            "agent": "codex",
            "event": "PermissionRequest",
            "pane_id": pane_id,
            "decision": {
                "id": "appr-perm-test",
                "prompt": "Allow tool execution?",
                "options": [
                    {"id": "opt_allow", "label": "Allow", "verdict": "allow"},
                    {"id": "opt_deny", "label": "Deny", "verdict": "deny"}
                ]
            }
        }),
    )
    .await
    .unwrap();

    // Check attention.pending includes this pane
    let pending = c
        .call("attention.pending", json!({"limit": 10}))
        .await
        .unwrap();
    let panes = pending["panes"].as_array().unwrap();
    let target = panes.iter().find(|p| p["pane_id"] == pane_id).unwrap();
    assert_eq!(target["attention"], "permission_required");
    assert_eq!(target["task_id"], task_id);

    // Answer decision
    let ans = c
        .call(
            "decision.answer",
            json!({
                "pane_id": pane_id,
                "decision_id": "appr-perm-test",
                "option_id": "opt_allow",
            }),
        )
        .await
        .unwrap();
    assert_eq!(ans["answered"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_start_ref_safety_rejects_flag_injection_and_invalid_branches() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // 1. Branch starting with dash
    let err = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Safety test"},
                "branch": "--malicious-flag",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap_err();
    assert!(err.starts_with(signaltty_proto::code::BAD_PARAMS));
    assert!(err.contains("invalid branch"), "got {err}");

    // 2. Base ref starting with dash
    let err2 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Safety test"},
                "base_ref": "--upload-pack=cat",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap_err();
    assert!(err2.starts_with(signaltty_proto::code::BAD_PARAMS));
    assert!(err2.contains("invalid base_ref"), "got {err2}");

    // 3. Invalid branch per git check-ref-format (contains ..)
    let err3 = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Safety test"},
                "branch": "invalid..branch",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap_err();
    assert!(err3.starts_with(signaltty_proto::code::BAD_PARAMS));
    assert!(err3.contains("invalid branch"), "got {err3}");

    srv.shutdown().await;
}

#[tokio::test]
async fn test_hook_receiver_drops_harness_mismatch_and_unparseable_event() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Start task with agent codex
    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Harness mismatch test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let pane_id = res["pane"]["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(res["task"]["worktree_path"].as_str().unwrap());

    // Fake agent drives worker to session_start as codex
    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt_path).await.unwrap();

    // Verify pane is codex
    let pane_get = c
        .call("pane.get", json!({"pane_id": &pane_id}))
        .await
        .unwrap();
    assert_eq!(pane_get["pane"]["agent"]["kind"], "codex");

    // 1. Cross-harness replay: Claude hook event sent to Codex pane
    let drop_mismatch = c
        .call(
            "hook-event",
            json!({
                "agent": "claude",
                "event": "Stop",
                "pane_id": &pane_id,
                "payload": {"session_id": "claude-sess-xyz"}
            }),
        )
        .await
        .unwrap();

    assert_eq!(drop_mismatch["dropped"], true);
    assert_eq!(drop_mismatch["reason"], "harness_mismatch");

    // Verify pane was NOT relabeled to claude and did not move to Done
    let pane_after = c
        .call("pane.get", json!({"pane_id": &pane_id}))
        .await
        .unwrap();
    assert_eq!(pane_after["pane"]["agent"]["kind"], "codex");

    // 2. Unparseable/unknown hook event: should be dropped, never default to Stop/Done
    let drop_unknown = c
        .call(
            "hook-event",
            json!({
                "agent": "codex",
                "event": "UnparseableGarbageHookXYZ",
                "pane_id": &pane_id,
                "payload": {}
            }),
        )
        .await
        .unwrap();

    assert_eq!(drop_unknown["dropped"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn test_last_message_capture_from_stop_and_permission_hooks() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Message capture test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = res["task"]["id"].as_str().unwrap().to_string();
    let pane_id = res["pane"]["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(res["task"]["worktree_path"].as_str().unwrap());

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt_path).await.unwrap();

    let wait = c
        .call(
            "task.wait",
            json!({
                "task_id": &task_id,
                "until": "working",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    // 1. PermissionRequest hook carries explicit prompt
    c.call(
        "hook-event",
        json!({
            "agent": "codex",
            "event": "PermissionRequest",
            "pane_id": &pane_id,
            "payload": {
                "session_id": driver.session_id,
                "prompt": "Allow reading /etc/passwd?",
            }
        }),
    )
    .await
    .unwrap();

    let att = c.call("attention.pending", json!({})).await.unwrap();
    let panes = att["panes"].as_array().unwrap();
    let pane_att = panes.iter().find(|p| p["pane_id"] == pane_id).unwrap();
    assert_eq!(pane_att["last_message"], "Allow reading /etc/passwd?");

    // 2. Stop hook carries last_assistant_message
    c.call(
        "hook-event",
        json!({
            "agent": "codex",
            "event": "Stop",
            "pane_id": &pane_id,
            "payload": {
                "session_id": driver.session_id,
                "last_assistant_message": "All unit tests pass and code is formatted.",
            }
        }),
    )
    .await
    .unwrap();

    let task_get = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_get["task"]["state"], "input_required");
    assert_eq!(
        task_get["task"]["status_reason"]["last_message"],
        "All unit tests pass and code is formatted."
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_silent_worker_watchdog_transitions_to_input_required() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start_with_env(&[("SIGNALTTY_WORKER_SILENT_TIMEOUT_S", "1")]).await;
    let mut c = srv.client().await;

    let res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Silent watchdog test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = res["task"]["id"].as_str().unwrap().to_string();
    let pane_id = res["pane"]["id"].as_str().unwrap().to_string();
    let wt_path = std::path::PathBuf::from(res["task"]["worktree_path"].as_str().unwrap());

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt_path).await.unwrap();

    let wait = c
        .call(
            "task.wait",
            json!({
                "task_id": &task_id,
                "until": "working",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    // Wait for watchdog to transition task to input_required with reason worker_silent
    let wait_settled = c
        .call(
            "task.wait",
            json!({
                "task_id": &task_id,
                "until": "settled",
                "timeout_s": 5,
            }),
        )
        .await
        .unwrap();
    assert_eq!(wait_settled["satisfied"], true);
    assert_eq!(wait_settled["tasks"][0]["state"], "input_required");

    let task_get = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(task_get["task"]["state"], "input_required");
    assert_eq!(task_get["task"]["status_reason"]["reason"], "worker_silent");

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_post_worktree_failure_leaves_failed_task_and_removable_checkout() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    // Unknown parent pane: the worktree is created before spawn is attempted,
    // so the failure must leave a failed task behind, not a bare error.
    let resp = c
        .call_raw_resp(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "will fail at spawn"},
                "label": "worker-fail",
                "parent_pane_id": "pane_nope",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    assert!(!resp.ok);
    let err = resp.error.unwrap();
    assert_eq!(err.code, signaltty_proto::code::NO_SUCH_PANE);
    let task_id = err.details["task_id"].as_str().unwrap().to_string();
    assert_eq!(err.details["stage"], "parent");

    let get = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(get["task"]["state"], "failed");
    assert_eq!(get["task"]["status_reason"]["stage"], "parent");
    let wt = PathBuf::from(get["task"]["worktree_path"].as_str().unwrap());
    assert!(wt.exists(), "failed task keeps its checkout on disk");

    // The checkout must be removable without a manual `git worktree unlock`.
    let out = repo.git(&["worktree", "remove", "--force", wt.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "locked checkout refuses removal: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_honors_submit_delay_ms() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "honor submit delay"},
                "agent": "codex",
                "argv": ["sh"],
                "submit_delay_ms": 1500,
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap();
    let pane_id = start["pane"]["id"].as_str().unwrap().to_string();
    let wt = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());

    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt).await.unwrap();
    let pane = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    assert_eq!(pane["pane"]["lifecycle"], "idle");

    tokio::time::sleep(Duration::from_millis(250)).await;
    let mid = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(
        mid["task"]["state"], "pending",
        "submit_delay_ms must hold the first write"
    );

    let wait = c
        .call(
            "task.wait",
            json!({"task_id": task_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);

    srv.shutdown().await;
}

#[tokio::test]
async fn task_start_rejects_unsendable_contract_before_worktree() {
    let repo = TempGitRepo::new();
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let worktrees =
        || String::from_utf8(repo.git(&["worktree", "list", "--porcelain"]).stdout).unwrap();
    let before = worktrees();

    let over = "x".repeat(signaltty_core::model::Contract::MAX_OBJECTIVE_BYTES + 1);
    let err = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": over},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(signaltty_proto::code::BAD_PARAMS),
        "oversize objective must be BAD_PARAMS before a worktree, got {err}"
    );
    assert_eq!(
        worktrees(),
        before,
        "oversize objective must create nothing"
    );

    let max = "y".repeat(signaltty_core::model::Contract::MAX_OBJECTIVE_BYTES);
    let err = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {
                    "objective": max,
                    "constraints": "z".repeat(4096),
                },
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(signaltty_proto::code::BAD_PARAMS),
        "composed prompt over the worker limit must be BAD_PARAMS, got {err}"
    );
    assert_eq!(
        worktrees(),
        before,
        "an unsendable composed prompt must create nothing"
    );

    let max = "y".repeat(signaltty_core::model::Contract::MAX_OBJECTIVE_BYTES);
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": max},
                "agent": "codex",
                "argv": ["sh"],
                "submit_delay_ms": 50,
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap();
    let pane_id = start["pane"]["id"].as_str().unwrap().to_string();
    let wt = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());
    let driver = FakeAgentPane::new(&pane_id, "codex");
    driver.session_start(&mut c, &wt).await.unwrap();
    let wait = c
        .call(
            "task.wait",
            json!({"task_id": task_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait["satisfied"], true);
    let got = c
        .call("task.get", json!({"task_id": task_id}))
        .await
        .unwrap();
    assert_eq!(got["task"]["state"], "working");
    assert!(got["task"]["status_reason"].is_null());

    srv.shutdown().await;
}
