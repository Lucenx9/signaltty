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
async fn orchestrator_deterministic_acceptance_scenario() {
    // 1. Create a temp git repo with one commit and an untouched pre-existing branch;
    //    start server and an orchestrator pane.
    let repo = TempGitRepo::new();
    repo.create_branch("pre-existing-feature");
    let pre_existing_sha_before =
        String::from_utf8_lossy(&repo.git(&["rev-parse", "pre-existing-feature"]).stdout)
            .trim()
            .to_string();

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
    let orch_pane_id = orch_p["pane"]["id"].as_str().unwrap().to_string();

    // 2. Orchestrator starts 3 tasks sharing one context, each with a contract,
    //    back-to-back (async: each returns at pending).
    let ctx_id = "tctx_deterministic_scenario";
    let start_a = c
        .call(
            "task.start",
            json!({
                "context_id": ctx_id,
                "repo": repo.path().to_string_lossy(),
                "branch": "task-a-branch",
                "parent_pane_id": orch_pane_id,
                "label": "worker-a",
                "agent": "codex",
                "argv": ["sleep", "60"],
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
                "repo": repo.path().to_string_lossy(),
                "branch": "task-b-branch",
                "parent_pane_id": orch_pane_id,
                "label": "worker-b",
                "agent": "codex",
                "argv": ["sleep", "60"],
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
                "repo": repo.path().to_string_lossy(),
                "branch": "task-c-branch",
                "parent_pane_id": orch_pane_id,
                "label": "worker-c",
                "agent": "codex",
                "argv": ["sleep", "60"],
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

    // 3. Worker panes have lineage (parent_pane_id, label) set.
    let pane_a_id = start_a["task"]["pane_id"].as_str().unwrap().to_string();
    let pane_b_id = start_b["task"]["pane_id"].as_str().unwrap().to_string();
    let pane_c_id = start_c["task"]["pane_id"].as_str().unwrap().to_string();

    let pane_a_info = c
        .call("pane.get", json!({"pane_id": pane_a_id}))
        .await
        .unwrap();
    assert_eq!(pane_a_info["pane"]["parent_pane_id"], orch_pane_id);
    assert_eq!(pane_a_info["pane"]["label"], "worker-a");

    let pane_b_info = c
        .call("pane.get", json!({"pane_id": pane_b_id}))
        .await
        .unwrap();
    assert_eq!(pane_b_info["pane"]["parent_pane_id"], orch_pane_id);
    assert_eq!(pane_b_info["pane"]["label"], "worker-b");

    let pane_c_info = c
        .call("pane.get", json!({"pane_id": pane_c_id}))
        .await
        .unwrap();
    assert_eq!(pane_c_info["pane"]["parent_pane_id"], orch_pane_id);
    assert_eq!(pane_c_info["pane"]["label"], "worker-c");

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

    // Worker B raises PermissionRequest -> attention/needs-user -> blocked -> input_required
    std::fs::write(worktree_b.join("file_b.txt"), "hello from B\n").unwrap();
    git(worktree_b, &["add", "file_b.txt"]);
    git(worktree_b, &["commit", "-qm", "feat: file_b"]);
    // Also add an untracked file to prove untracked diff detection
    std::fs::write(worktree_b.join("untracked_b.txt"), "untracked\n").unwrap();

    let mut hook_b = srv.client().await;
    let b_pid = pane_b_id.clone();
    let b_sid = driver_b.session_id.clone();
    let _perm_task = tokio::spawn(async move {
        hook_b
            .call(
                "hook-event",
                json!({
                    "agent": "codex",
                    "event": "PermissionRequest",
                    "pane_id": b_pid,
                    "wait_for_answer": true,
                    "wait_timeout_s": 10,
                    "payload": {
                        "session_id": b_sid,
                        "tool_name": "Bash",
                        "tool_input": {"command": "touch file_b.txt"},
                    }
                }),
            )
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(60)).await;

    // Poll until the blocked worker shows up (bounded; failure is explicit).
    let mut found_b = false;
    for _ in 0..100 {
        let pending_att = c.call("attention.pending", json!({})).await.unwrap();
        let items = pending_att["panes"].as_array().unwrap();
        if items.iter().any(|item| item["pane_id"] == pane_b_id) {
            found_b = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found_b, "worker B must appear in attention.pending");

    let task_b_blocked = c
        .call("task.get", json!({"task_id": task_b_id}))
        .await
        .unwrap();
    assert_eq!(task_b_blocked["task"]["state"], "input_required");

    let pane_b_latest = c
        .call("pane.get", json!({"pane_id": pane_b_id}))
        .await
        .unwrap();
    let dec = &pane_b_latest["pane"]["pending_decision"];
    let dec_id = dec["id"].as_str().unwrap();
    let opt_id = dec["options"][0]["id"].as_str().unwrap();
    c.call(
        "decision.answer",
        json!({"pane_id": pane_b_id, "decision_id": dec_id, "option_id": opt_id}),
    )
    .await
    .unwrap();

    let task_b_working = c
        .call("task.get", json!({"task_id": task_b_id}))
        .await
        .unwrap();
    assert_eq!(task_b_working["task"]["state"], "working");

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
    let tasks_1 = wait_ctx_1["tasks"].as_array().unwrap();
    let task_c = tasks_1.iter().find(|t| t["id"] == task_c_id).unwrap();
    assert_eq!(task_c["state"], "input_required");
    assert_eq!(
        task_c["status_reason"]["reason"],
        "turn_ended_without_report"
    );

    // Fake agent C responds to the submit with UserPromptSubmit
    let sock_c = srv.socket.clone();
    let pid_c_cl = pane_c_id.clone();
    let sess_c_cl = driver_c.session_id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if let Ok(mut c2) = signaltty_testkit::TestClient::connect(&sock_c).await {
            let _ = c2
                .call(
                    "hook-event",
                    json!({
                        "agent": "codex",
                        "event": "UserPromptSubmit",
                        "pane_id": pid_c_cl,
                        "payload": {"session_id": sess_c_cl}
                    }),
                )
                .await;
        }
    });

    // Orchestrator sends follow-up submit to worker C -> task back to working.
    let submit_c = c
        .call(
            "pane.submit",
            json!({"pane_id": pane_c_id, "text": "Please report your result"}),
        )
        .await
        .unwrap();
    assert_eq!(submit_c["submitted"], true);

    let get_c = c
        .call("task.get", json!({"task_id": task_c_id}))
        .await
        .unwrap();
    assert_eq!(get_c["task"]["state"], "working");

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
    let tasks_2 = wait_ctx_2["tasks"].as_array().unwrap();
    assert!(tasks_2.iter().all(|t| {
        let st = t["state"].as_str().unwrap_or("");
        ["completed", "failed", "canceled", "rejected"].contains(&st)
    }));

    // Incremental output reads on worker A.
    let read_1 = c
        .call(
            "pane.read",
            json!({"pane_id": pane_a_id, "mode": "rendered", "after_seq": 0}),
        )
        .await
        .unwrap();
    let seq_1 = read_1["next_seq"].as_u64().unwrap_or(0);

    // Cause output in pane A between two reads
    c.call(
        "pane.input",
        json!({
            "pane_id": pane_a_id,
            "data_b64": "ZWNobyBpbmNyZW1lbnRhbF9vdXRwdXRfYQo="
        }),
    )
    .await
    .unwrap();
    // Poll for the PTY-echoed marker (bounded; failure is explicit).
    let mut text_2 = String::new();
    let mut read_2 = serde_json::json!({});
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        read_2 = c
            .call(
                "pane.read",
                json!({"pane_id": pane_a_id, "mode": "rendered", "after_seq": seq_1}),
            )
            .await
            .unwrap();
        text_2 = read_2["text"].as_str().unwrap_or("").to_string();
        if text_2.contains("incremental_output_a") {
            break;
        }
    }
    assert_eq!(read_2["dropped"], false);
    assert!(
        text_2.contains("incremental_output_a"),
        "second read must contain only the new output"
    );
    assert!(!text_2.contains("Worker A done"));

    // Needs-user attention listing is now empty.
    let att_final = c.call("attention.pending", json!({})).await.unwrap();
    assert!(att_final["panes"].as_array().unwrap().is_empty());

    // 6. Diff each task vs recorded base.
    let diff_a = c
        .call("task.diff", json!({"task_id": task_a_id}))
        .await
        .unwrap();
    let files_a = diff_a["files"].as_array().unwrap();
    assert!(files_a.iter().any(|f| f["path"] == "file_a.txt"));

    let diff_b = c
        .call("task.diff", json!({"task_id": task_b_id}))
        .await
        .unwrap();
    let files_b = diff_b["files"].as_array().unwrap();
    assert!(files_b.iter().any(|f| f["path"] == "file_b.txt"));
    assert!(files_b.iter().any(|f| f["path"] == "untracked_b.txt"));

    let diff_c = c
        .call("task.diff", json!({"task_id": task_c_id}))
        .await
        .unwrap();
    let files_c = diff_c["files"].as_array().unwrap();
    assert!(files_c.iter().any(|f| f["path"] == "file_c.txt"));

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
    let files_a_restart = diff_a_restart["files"].as_array().unwrap();
    assert!(files_a_restart.iter().any(|f| f["path"] == "file_a.txt"));

    // 8. Orchestrator finishes: merge 2 (A and B), discard 1 (C).
    let fin_a = c_after
        .call(
            "task.finish",
            json!({"task_id": task_a_id, "mode": "merge", "delete_branch": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_a["task"]["disposition"]["outcome"], "merged");

    let fin_b = c_after
        .call(
            "task.finish",
            json!({"task_id": task_b_id, "mode": "merge", "delete_branch": true, "ignore_dirty": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_b["task"]["disposition"]["outcome"], "merged");

    let fin_c = c_after
        .call(
            "task.finish",
            json!({"task_id": task_c_id, "mode": "discard", "delete_branch": true}),
        )
        .await
        .unwrap();
    assert_eq!(fin_c["task"]["disposition"]["outcome"], "discarded");

    // Merged files land in the source repo.
    let main_a = repo.path().join("file_a.txt");
    let main_b = repo.path().join("file_b.txt");
    assert!(main_a.exists(), "file_a.txt merged into source repo");
    assert!(main_b.exists(), "file_b.txt merged into source repo");

    // All worktrees are removed.
    assert!(!worktree_a.exists(), "worktree A removed");
    assert!(!worktree_b.exists(), "worktree B removed");
    assert!(!worktree_c.exists(), "discarded worktree C is deleted");

    // Task branches deleted only where allowed.
    let br_a = repo.git(&["branch", "--list", "task-a-branch"]);
    assert!(
        String::from_utf8_lossy(&br_a.stdout).trim().is_empty(),
        "task-a-branch was deleted"
    );
    let br_b = repo.git(&["branch", "--list", "task-b-branch"]);
    assert!(
        String::from_utf8_lossy(&br_b.stdout).trim().is_empty(),
        "task-b-branch was deleted"
    );
    let br_c = repo.git(&["branch", "--list", "task-c-branch"]);
    assert!(
        String::from_utf8_lossy(&br_c.stdout).trim().is_empty(),
        "task-c-branch was deleted"
    );

    // Source repo is on its target branch and clean.
    let current_branch = repo.git(&["branch", "--show-current"]);
    assert_eq!(
        String::from_utf8_lossy(&current_branch.stdout).trim(),
        "main",
        "source repo is on its target branch"
    );
    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "source repo is clean after finish"
    );

    // Pre-existing branch still points at its original SHA.
    let pre_existing_sha_after =
        String::from_utf8_lossy(&repo.git(&["rev-parse", "pre-existing-feature"]).stdout)
            .trim()
            .to_string();
    assert_eq!(
        pre_existing_sha_before, pre_existing_sha_after,
        "pre-existing branch still points at original SHA"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_restart_with_mixed_task_states_and_pane_kill_idempotent() {
    let repo = TempGitRepo::new();
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;

    // 1. Task Completed: starts, drives to working, writes file, reports completed
    let start_done = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "agent": "codex",
                "argv": ["sleep", "60"],
                "contract": {"objective": "Done task"}
            }),
        )
        .await
        .unwrap();
    let task_done_id = start_done["task"]["id"].as_str().unwrap().to_string();
    let pane_done_id = start_done["pane"]["id"].as_str().unwrap().to_string();
    let wt_done = std::path::PathBuf::from(start_done["task"]["worktree_path"].as_str().unwrap());

    let driver_done = FakeAgentPane::new(&pane_done_id, "codex");
    driver_done.session_start(&mut c, &wt_done).await.unwrap();

    let _ = c
        .call(
            "task.wait",
            json!({"task_id": task_done_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();

    std::fs::write(
        wt_done.join("done.txt"),
        "done
",
    )
    .unwrap();
    let rep = c
        .call(
            "task.report",
            json!({
                "task_id": task_done_id,
                "status": "completed",
                "summary": "Finished nicely",
                "artifacts": [{"name": "done.txt", "path": "done.txt"}]
            }),
        )
        .await
        .unwrap();
    assert_eq!(rep["task"]["state"], "completed");

    // 2. Task Working: starts, drives to working, writes file
    let start_working = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "agent": "codex",
                "argv": ["sleep", "60"],
                "contract": {"objective": "Working task"}
            }),
        )
        .await
        .unwrap();
    let task_working_id = start_working["task"]["id"].as_str().unwrap().to_string();
    let pane_working_id = start_working["pane"]["id"].as_str().unwrap().to_string();
    let wt_working =
        std::path::PathBuf::from(start_working["task"]["worktree_path"].as_str().unwrap());

    let driver_working = FakeAgentPane::new(&pane_working_id, "codex");
    driver_working
        .session_start(&mut c, &wt_working)
        .await
        .unwrap();
    let _ = c
        .call(
            "task.wait",
            json!({"task_id": task_working_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();

    std::fs::write(
        wt_working.join("wip.txt"),
        "in progress
",
    )
    .unwrap();

    // 3. Task InputRequired: starts, drives to working, Stop hook moves to input_required
    let start_ir = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "agent": "codex",
                "argv": ["sleep", "60"],
                "contract": {"objective": "Input required task"}
            }),
        )
        .await
        .unwrap();
    let task_ir_id = start_ir["task"]["id"].as_str().unwrap().to_string();
    let pane_ir_id = start_ir["pane"]["id"].as_str().unwrap().to_string();
    let wt_ir = std::path::PathBuf::from(start_ir["task"]["worktree_path"].as_str().unwrap());

    let driver_ir = FakeAgentPane::new(&pane_ir_id, "codex");
    driver_ir.session_start(&mut c, &wt_ir).await.unwrap();
    let _ = c
        .call(
            "task.wait",
            json!({"task_id": task_ir_id, "until": "working", "timeout_s": 5}),
        )
        .await
        .unwrap();

    c.call(
        "hook-event",
        json!({
            "agent": "codex",
            "event": "Stop",
            "pane_id": pane_ir_id,
            "message": "Turn finished without reporting"
        }),
    )
    .await
    .unwrap();

    let get_ir = c
        .call("task.get", json!({"task_id": task_ir_id}))
        .await
        .unwrap();
    assert_eq!(get_ir["task"]["state"], "input_required");

    // 4. Task Pending: starts, we do NOT drive it (remains pending until restart)
    let start_pending = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "agent": "codex",
                "argv": ["sleep", "60"],
                "contract": {"objective": "Pending task"}
            }),
        )
        .await
        .unwrap();
    let task_pending_id = start_pending["task"]["id"].as_str().unwrap().to_string();
    let wt_pending =
        std::path::PathBuf::from(start_pending["task"]["worktree_path"].as_str().unwrap());
    assert_eq!(start_pending["task"]["state"], "pending");

    // 5. Pane kill idempotency: start a task, kill its pane, verify fail exactly once
    let start_kill = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "agent": "codex",
                "argv": ["sleep", "60"],
                "contract": {"objective": "Kill task"}
            }),
        )
        .await
        .unwrap();
    let task_kill_id = start_kill["task"]["id"].as_str().unwrap().to_string();
    let pane_kill_id = start_kill["pane"]["id"].as_str().unwrap().to_string();

    // Close pane (killing it)
    c.call("pane.close", json!({"pane_id": pane_kill_id}))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let kill_get_1 = c
        .call("task.get", json!({"task_id": task_kill_id}))
        .await
        .unwrap();
    assert_eq!(kill_get_1["task"]["state"], "failed");
    let reason_1 = kill_get_1["task"]["status_reason"].clone();
    assert!(!reason_1.is_null());

    // Calling pane.close again or redundant exit does not error or change task state
    let close_again = c.call("pane.close", json!({"pane_id": pane_kill_id})).await;
    // pane is already closed so NO_SUCH_PANE is returned
    assert!(close_again.is_err());
    let kill_get_2 = c
        .call("task.get", json!({"task_id": task_kill_id}))
        .await
        .unwrap();
    assert_eq!(kill_get_2["task"]["state"], "failed");
    assert_eq!(kill_get_2["task"]["status_reason"], reason_1);

    // 6. RESTART SERVER
    srv.restart().await;
    let mut c2 = srv.client().await;

    // 7. Verify all tasks after restart:
    // 7a. Task Completed: intact, results preserved, diffs work!
    let post_done = c2
        .call("task.get", json!({"task_id": task_done_id}))
        .await
        .unwrap();
    assert_eq!(post_done["task"]["state"], "completed");
    assert_eq!(post_done["task"]["result"]["summary"], "Finished nicely");

    let diff_done = c2
        .call("task.diff", json!({"task_id": task_done_id}))
        .await
        .unwrap();
    let files_done = diff_done["files"].as_array().unwrap();
    assert!(files_done.iter().any(|f| f["path"] == "done.txt"));

    // 7b. Task Working: failed with restart stage/evidence, checkout preserved
    let post_working = c2
        .call("task.get", json!({"task_id": task_working_id}))
        .await
        .unwrap();
    assert_eq!(post_working["task"]["state"], "failed");
    assert_eq!(post_working["task"]["status_reason"]["stage"], "restart");
    assert!(
        wt_working.exists(),
        "working task checkout must be preserved for post-mortem"
    );
    assert!(
        wt_working.join("wip.txt").exists(),
        "working files must remain intact"
    );

    // 7c. Task InputRequired: failed with restart stage/evidence, checkout preserved
    let post_ir = c2
        .call("task.get", json!({"task_id": task_ir_id}))
        .await
        .unwrap();
    assert_eq!(post_ir["task"]["state"], "failed");
    assert_eq!(post_ir["task"]["status_reason"]["stage"], "restart");
    assert!(
        wt_ir.exists(),
        "input_required task checkout must be preserved"
    );

    // 7d. Task Pending: failed with restart stage/evidence, checkout preserved
    let post_pending = c2
        .call("task.get", json!({"task_id": task_pending_id}))
        .await
        .unwrap();
    assert_eq!(post_pending["task"]["state"], "failed");
    assert_eq!(post_pending["task"]["status_reason"]["stage"], "restart");
    assert!(
        wt_pending.exists(),
        "pending task checkout must be preserved"
    );

    srv.shutdown().await;
}
