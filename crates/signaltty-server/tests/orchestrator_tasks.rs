use std::path::PathBuf;
use std::time::Duration;

use chrono::Utc;
use serde_json::json;
use signaltty_core::model::{
    Contract, Disposition, DispositionOutcome, Relationship, Task, TaskResult, TaskResultStatus,
};
use signaltty_core::state::TaskState;
use signaltty_testkit::TestServer;

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
