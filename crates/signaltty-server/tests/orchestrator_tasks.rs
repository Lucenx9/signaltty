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
                "context_id": "ctx_happy"
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
