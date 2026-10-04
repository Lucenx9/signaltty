use std::path::Path;

use serde_json::json;
use signaltty_testkit::{FakeAgentPane, TempGitRepo, TestServer};

fn git(cwd: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {:?}",
        out.stderr
    );
}

#[tokio::test]
#[ignore = "018: greens in Phase 7"]
async fn orchestrator_deterministic_acceptance_scenario() {
    // 1. Create a temp git repo with one commit and an untouched pre-existing branch;
    //    start server and an orchestrator pane.
    let repo = TempGitRepo::new();
    repo.create_branch("pre-existing-feature");

    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": repo.path().to_string_lossy(), "name": "orch-ws"}),
        )
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let orch_p = c
        .call("pane.spawn", json!({"workspace_id": ws_id, "argv": ["sh"]}))
        .await
        .unwrap();
    let _orch_pane_id = orch_p["pane"]["id"].as_str().unwrap();

    // 2. Orchestrator starts 3 tasks sharing one context, each with a contract,
    //    back-to-back (async: each returns at pending).
    let ctx_id = "tctx_deterministic_scenario";
    let start_a = c
        .call(
            "task.start",
            json!({
                "context_id": ctx_id,
                "source_repo": repo.path().to_string_lossy(),
                "branch": "task-a-branch",
                "contract": {
                    "objective": "Worker A objective: create file_a.txt",
                    "acceptance_criteria": ["file_a.txt exists"]
                }
            }),
        )
        .await
        .unwrap();
    let task_a_id = start_a["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(start_a["task"]["state"], "pending");

    let start_b = c
        .call(
            "task.start",
            json!({
                "context_id": ctx_id,
                "source_repo": repo.path().to_string_lossy(),
                "branch": "task-b-branch",
                "contract": {
                    "objective": "Worker B objective: create file_b.txt with permission",
                    "acceptance_criteria": ["file_b.txt exists"]
                }
            }),
        )
        .await
        .unwrap();
    let task_b_id = start_b["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(start_b["task"]["state"], "pending");

    let start_c = c
        .call(
            "task.start",
            json!({
                "context_id": ctx_id,
                "source_repo": repo.path().to_string_lossy(),
                "branch": "task-c-branch",
                "contract": {
                    "objective": "Worker C objective: finish after follow-up",
                    "acceptance_criteria": ["file_c.txt exists"]
                }
            }),
        )
        .await
        .unwrap();
    let task_c_id = start_c["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(start_c["task"]["state"], "pending");

    // 3. Each worker pane becomes ready (SessionStart hook -> idle).
    let pane_a_id = start_a["task"]["pane_id"].as_str().unwrap().to_string();
    let pane_b_id = start_b["task"]["pane_id"].as_str().unwrap().to_string();
    let pane_c_id = start_c["task"]["pane_id"].as_str().unwrap().to_string();

    let worktree_a = Path::new(start_a["task"]["worktree_path"].as_str().unwrap());
    let worktree_b = Path::new(start_b["task"]["worktree_path"].as_str().unwrap());
    let worktree_c = Path::new(start_c["task"]["worktree_path"].as_str().unwrap());

    let driver_a = FakeAgentPane::new(&pane_a_id, "codex");
    let driver_b = FakeAgentPane::new(&pane_b_id, "codex");
    let driver_c = FakeAgentPane::new(&pane_c_id, "codex");

    driver_a.session_start(&mut c, worktree_a).await.unwrap();
    driver_b.session_start(&mut c, worktree_b).await.unwrap();
    driver_c.session_start(&mut c, worktree_c).await.unwrap();

    // The background ready-wait submits objectives and all three reach working.
    let wait_a = c
        .call(
            "task.wait",
            json!({"task_id": task_a_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait_a["satisfied"], true);

    let wait_b = c
        .call(
            "task.wait",
            json!({"task_id": task_b_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait_b["satisfied"], true);

    let wait_c = c
        .call(
            "task.wait",
            json!({"task_id": task_c_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();
    assert_eq!(wait_c["satisfied"], true);

    // 4. Workers turn working (UserPromptSubmit).
    driver_a.prompt_submit(&mut c).await.unwrap();
    driver_b.prompt_submit(&mut c).await.unwrap();
    driver_c.prompt_submit(&mut c).await.unwrap();

    // Worker A makes changes and reports completed.
    std::fs::write(worktree_a.join("file_a.txt"), "hello from A\n").unwrap();
    git(worktree_a, &["add", "file_a.txt"]);
    git(worktree_a, &["commit", "-qm", "feat: file_a"]);

    let rep_a = c
        .call(
            "task.report",
            json!({
                "task_id": task_a_id,
                "status": "completed",
                "summary": "Worker A done: created file_a.txt",
                "artifacts": [{"name": "file_a.txt", "path": "file_a.txt"}]
            }),
        )
        .await
        .unwrap();
    assert_eq!(rep_a["task"]["state"], "completed");

    // Worker B raises PermissionRequest -> attention/needs-user -> answered -> report completed.
    std::fs::write(worktree_b.join("file_b.txt"), "hello from B\n").unwrap();
    git(worktree_b, &["add", "file_b.txt"]);
    git(worktree_b, &["commit", "-qm", "feat: file_b"]);
    // Also add an untracked file to prove untracked diff detection
    std::fs::write(worktree_b.join("untracked_b.txt"), "untracked\n").unwrap();

    driver_b
        .permission_request(&mut c, "Bash", json!({"command": "touch file_b.txt"}))
        .await
        .unwrap();

    let pending_att = c.call("attention.pending", json!({})).await.unwrap();
    let items = pending_att["panes"].as_array().unwrap();
    assert!(items.iter().any(|item| item["id"] == pane_b_id));

    let pane_b_info = c
        .call("pane.get", json!({"pane_id": pane_b_id}))
        .await
        .unwrap();
    let dec_id = pane_b_info["pane"]["pending_decision"]["id"]
        .as_str()
        .unwrap();
    c.call(
        "decision.answer",
        json!({"decision_id": dec_id, "verdict": "allow"}),
    )
    .await
    .unwrap();

    let rep_b = c
        .call(
            "task.report",
            json!({
                "task_id": task_b_id,
                "status": "completed",
                "summary": "Worker B done: created file_b.txt",
                "artifacts": [{"name": "file_b.txt", "path": "file_b.txt"}]
            }),
        )
        .await
        .unwrap();
    assert_eq!(rep_b["task"]["state"], "completed");

    // Worker C ends turn WITHOUT reporting -> task moves to input_required.
    std::fs::write(worktree_c.join("file_c.txt"), "hello from C\n").unwrap();
    git(worktree_c, &["add", "file_c.txt"]);
    git(worktree_c, &["commit", "-qm", "feat: file_c"]);

    driver_c.stop(&mut c).await.unwrap();

    // 5. Orchestrator waits on the context with default settled until
    //    wait ends at worker C's input_required.
    let wait_ctx_1 = c
        .call("task.wait", json!({"context_id": ctx_id, "timeout_s": 5}))
        .await
        .unwrap();
    assert_eq!(wait_ctx_1["satisfied"], true);
    assert_eq!(wait_ctx_1["settled_task"]["id"], task_c_id);
    assert_eq!(wait_ctx_1["settled_task"]["state"], "input_required");
    assert_eq!(
        wait_ctx_1["settled_task"]["finish_error"]["reason"],
        "turn_ended_without_report"
    );

    // Orchestrator sends follow-up submit to worker C -> task back to working.
    let submit_c = c
        .call(
            "pane.submit",
            json!({"pane_id": pane_c_id, "text": "Please report your result"}),
        )
        .await
        .unwrap();
    assert_eq!(submit_c["task"]["state"], "working");

    // Worker C reports completed.
    let rep_c = c
        .call(
            "task.report",
            json!({
                "task_id": task_c_id,
                "status": "completed",
                "summary": "Worker C done: follow-up answered",
                "artifacts": [{"name": "file_c.txt", "path": "file_c.txt"}]
            }),
        )
        .await
        .unwrap();
    assert_eq!(rep_c["task"]["state"], "completed");

    // Second context wait: all terminal.
    let wait_ctx_2 = c
        .call("task.wait", json!({"context_id": ctx_id, "timeout_s": 5}))
        .await
        .unwrap();
    assert_eq!(wait_ctx_2["satisfied"], true);
    assert_eq!(wait_ctx_2["all_terminal"], true);

    // Incremental output reads on worker A.
    let read_1 = c
        .call(
            "pane.read",
            json!({"pane_id": pane_a_id, "mode": "rendered", "after_offset": 0}),
        )
        .await
        .unwrap();
    let offset_1 = read_1["next_offset"].as_u64().unwrap_or(0);
    let read_2 = c
        .call(
            "pane.read",
            json!({"pane_id": pane_a_id, "mode": "rendered", "after_offset": offset_1}),
        )
        .await
        .unwrap();
    assert!(read_2["text"].as_str().unwrap_or("").is_empty());

    // Needs-user attention listing is now empty.
    let att_final = c.call("attention.pending", json!({})).await.unwrap();
    assert!(att_final["panes"].as_array().unwrap().is_empty());

    // 6. Diff each task vs recorded base.
    let diff_a = c
        .call("task.diff", json!({"task_id": task_a_id}))
        .await
        .unwrap();
    assert!(diff_a["diff"].as_str().unwrap().contains("file_a.txt"));

    let diff_b = c
        .call("task.diff", json!({"task_id": task_b_id}))
        .await
        .unwrap();
    assert!(diff_b["diff"].as_str().unwrap().contains("file_b.txt"));
    assert!(diff_b["diff"].as_str().unwrap().contains("untracked_b.txt"));

    let diff_c = c
        .call("task.diff", json!({"task_id": task_c_id}))
        .await
        .unwrap();
    assert!(diff_c["diff"].as_str().unwrap().contains("file_c.txt"));

    // 7. Server restarts mid-run (here after all reports).
    srv.restart().await;
    let mut c_after = srv.client().await;

    // All 3 tasks still completed with results intact; diffs still work.
    let get_a = c_after
        .call("task.get", json!({"task_id": task_a_id}))
        .await
        .unwrap();
    assert_eq!(get_a["task"]["state"], "completed");
    assert_eq!(
        get_a["task"]["result"]["summary"],
        "Worker A done: created file_a.txt"
    );

    let get_b = c_after
        .call("task.get", json!({"task_id": task_b_id}))
        .await
        .unwrap();
    assert_eq!(get_b["task"]["state"], "completed");

    let get_c = c_after
        .call("task.get", json!({"task_id": task_c_id}))
        .await
        .unwrap();
    assert_eq!(get_c["task"]["state"], "completed");

    let diff_a_restart = c_after
        .call("task.diff", json!({"task_id": task_a_id}))
        .await
        .unwrap();
    assert!(diff_a_restart["diff"]
        .as_str()
        .unwrap()
        .contains("file_a.txt"));

    // 8. Orchestrator finishes: merge 2 (A and B), discard 1 (C).
    let fin_a = c_after
        .call(
            "task.finish",
            json!({"task_id": task_a_id, "action": "merge", "delete_branch": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_a["disposition"]["outcome"], "merged");

    let fin_b = c_after
        .call(
            "task.finish",
            json!({"task_id": task_b_id, "action": "merge", "delete_branch": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_b["disposition"]["outcome"], "merged");

    let fin_c = c_after
        .call(
            "task.finish",
            json!({"task_id": task_c_id, "action": "discard", "delete_branch": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_c["disposition"]["outcome"], "discarded");

    // Merged files land in the source repo.
    let main_a = repo.path().join("file_a.txt");
    let main_b = repo.path().join("file_b.txt");
    assert!(main_a.exists(), "file_a.txt merged into source repo");
    assert!(main_b.exists(), "file_b.txt merged into source repo");

    // Discarded worktree is gone.
    assert!(!worktree_c.exists(), "discarded worktree C is deleted");

    // Pre-existing branch is untouched.
    let branches = repo.git(&["branch", "--list", "pre-existing-feature"]);
    assert!(
        String::from_utf8_lossy(&branches.stdout).contains("pre-existing-feature"),
        "pre-existing branch was not touched"
    );

    srv.shutdown().await;
}
