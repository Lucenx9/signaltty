use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use signaltty_proto::code;
use signaltty_testkit::{TempGitRepo, TestServer};

#[tokio::test]
async fn test_task_diff_and_file_diff_vs_base_sha_untracked_and_clean_index() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Test diff and file_diff"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());
    let base_sha = start_res["task"]["base_sha"].as_str().unwrap().to_string();

    // 1. Worker commits a tracked file change
    std::fs::write(wt_path.join("README.md"), "updated in task commit\n").unwrap();
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &wt_path.to_string_lossy(),
            "commit",
            "-am",
            "task commit",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "commit failed: {:?}", out.stderr);

    // 2. Worker makes an unstaged tracked change
    std::fs::write(
        wt_path.join("README.md"),
        "updated in task commit\nand unstaged addition\n",
    )
    .unwrap();

    // 3. Worker creates an untracked file
    std::fs::write(wt_path.join("untracked.txt"), "hello untracked\nline 2\n").unwrap();

    // Record porcelain output before diff
    let pre_porcelain = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .unwrap();
    let pre_status = String::from_utf8_lossy(&pre_porcelain.stdout).to_string();

    // Call task.diff
    let diff_res = c
        .call("task.diff", json!({ "task_id": &task_id }))
        .await
        .unwrap();

    assert_eq!(diff_res["task_id"], task_id);
    assert_eq!(diff_res["base_sha"], base_sha);
    assert_eq!(diff_res["files"].as_array().unwrap().len(), 2); // README.md and untracked.txt
    assert!(diff_res["added"].as_u64().unwrap() >= 3);

    // Verify git index was untouched (no `git add -N`)
    let post_porcelain = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .unwrap();
    let post_status = String::from_utf8_lossy(&post_porcelain.stdout).to_string();
    assert_eq!(
        pre_status, post_status,
        "task.diff must not mutate git index or porcelain status"
    );

    // Call task.file_diff on tracked file
    let fd_tracked = c
        .call(
            "task.file_diff",
            json!({ "task_id": &task_id, "path": "README.md" }),
        )
        .await
        .unwrap();
    assert_eq!(fd_tracked["task_id"], task_id);
    assert_eq!(fd_tracked["path"], "README.md");
    assert_eq!(fd_tracked["untracked"], false);
    assert_eq!(fd_tracked["content"]["kind"], "text");

    // Call task.file_diff on untracked file
    let fd_untracked = c
        .call(
            "task.file_diff",
            json!({ "task_id": &task_id, "path": "untracked.txt" }),
        )
        .await
        .unwrap();
    assert_eq!(fd_untracked["task_id"], task_id);
    assert_eq!(fd_untracked["path"], "untracked.txt");
    assert_eq!(fd_untracked["untracked"], true);
    assert_eq!(fd_untracked["content"]["kind"], "text");

    // Unknown task returns NO_SUCH_TASK
    let err = c
        .call("task.diff", json!({ "task_id": "nonexistent" }))
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::NO_SUCH_TASK),
        "expected NO_SUCH_TASK, got: {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_merge_happy_path_and_disposition() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Merge happy path"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());

    // Worker creates a commit on the task branch
    std::fs::write(wt_path.join("feature.txt"), "feature content\n").unwrap();
    let out = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "add", "feature.txt"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &wt_path.to_string_lossy(),
            "commit",
            "-m",
            "feature commit",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Finish requires task to be completed first
    let premature_finish = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        premature_finish.starts_with(code::BAD_PARAMS),
        "finishing non-completed task must fail: {premature_finish}"
    );

    // Report completed
    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done feature"
        }),
    )
    .await
    .unwrap();
    let diff = c
        .call("task.diff", json!({ "task_id": &task_id }))
        .await
        .unwrap();
    assert_eq!(diff["merge_preview"]["clean"], true, "{diff}");

    // Call task.finish with merge
    let finish_res = c
        .call(
            "task.finish",
            json!({
                "task_id": &task_id,
                "mode": "merge",
            }),
        )
        .await
        .unwrap();

    assert_eq!(finish_res["task"]["disposition"]["outcome"], "merged");
    assert!(finish_res["merge"]["sha"].is_string());
    assert_eq!(finish_res["merge"]["target"], "main");

    // Target repo (main) now contains feature.txt
    assert!(repo.path().join("feature.txt").exists());
    let content = std::fs::read_to_string(repo.path().join("feature.txt")).unwrap();
    assert_eq!(content, "feature content\n");

    // Task worktree was cleaned up
    assert!(
        !wt_path.exists(),
        "worktree must be removed after successful finish merge"
    );

    // The closed worker leaves nothing in the attention list, even after
    // the PTY's late exit lands.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let pane_id = start_res["pane"]["id"].as_str().unwrap();
    let pending = c.call("attention.pending", json!({})).await.unwrap();
    assert!(
        !pending["panes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["pane_id"] == pane_id),
        "finished worker still listed: {pending}"
    );

    // A merged task branch is deleted by default.
    assert_eq!(finish_res["task"]["disposition"]["branch_deleted"], true);
    let branch = finish_res["task"]["branch"].as_str().unwrap();
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &repo.path().to_string_lossy(),
            "branch",
            "--list",
            branch,
        ])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).trim().is_empty());

    // Second finish must be refused
    let second_finish = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        second_finish.starts_with(code::BAD_PARAMS),
        "second finish must fail: {second_finish}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_merge_conflict_aborts_target_clean_and_names_files() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    // Base file in main
    std::fs::write(repo.path().join("conflict.txt"), "line A\n").unwrap();
    repo.git(&["add", "conflict.txt"]);
    repo.git(&["commit", "-m", "base conflict.txt"]);

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Conflict test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());

    // Advance main with competing commit
    std::fs::write(repo.path().join("conflict.txt"), "line in main branch\n").unwrap();
    repo.git(&["commit", "-am", "competing commit in main"]);

    // In task worktree, commit different change to same file
    std::fs::write(wt_path.join("conflict.txt"), "line in task branch\n").unwrap();
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &wt_path.to_string_lossy(),
            "commit",
            "-am",
            "competing in task",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());

    // Report completed
    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Ready to merge"
        }),
    )
    .await
    .unwrap();

    // The conflict is visible before finishing.
    let diff = c
        .call("task.diff", json!({ "task_id": &task_id }))
        .await
        .unwrap();
    assert_eq!(diff["merge_preview"]["clean"], false, "{diff}");
    assert_eq!(diff["merge_preview"]["conflicted"], json!(["conflict.txt"]));

    // Call task.finish with merge
    let finish_err = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();

    assert!(
        finish_err.starts_with(code::MERGE_CONFLICT),
        "expected MERGE_CONFLICT error, got: {finish_err}"
    );

    // Target (main) must be porcelain clean and have aborted merge
    let status_out = repo.git(&["status", "--porcelain"]);
    let status_str = String::from_utf8_lossy(&status_out.stdout);
    assert_eq!(
        status_str.trim(),
        "",
        "target repository must be clean after conflict abort"
    );

    // Task worktree must still exist with resolution opportunity
    assert!(
        wt_path.exists(),
        "source worktree must be preserved on conflict"
    );

    // Conflict files are stored on the task so a later status read shows them.
    // Disposition stays unset so finish can be retried after the conflict is resolved.
    let get = c
        .call("task.get", json!({"task_id": &task_id}))
        .await
        .unwrap();
    assert_eq!(get["task"]["disposition"]["outcome"], "none");
    let conflicted = get["task"]["finish_error"]["conflicted"]
        .as_array()
        .expect("finish_error.conflicted must be stored");
    assert!(
        conflicted
            .iter()
            .any(|f| f.as_str() == Some("conflict.txt")),
        "finish_error must name conflict.txt, got {conflicted:?}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_dirty_source_and_target_refusals() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Dirty test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());

    // Commit one change
    std::fs::write(wt_path.join("file1.txt"), "committed\n").unwrap();
    let _ = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "add", "file1.txt"])
        .output();
    let _ = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "commit", "-m", "commit 1"])
        .output();

    // Complete task
    c.call(
        "task.report",
        json!({ "task_id": &task_id, "status": "completed", "summary": "Done" }),
    )
    .await
    .unwrap();

    // Leave dirty uncommitted file in source worktree
    std::fs::write(wt_path.join("uncommitted.txt"), "dirt\n").unwrap();

    // Finish merge should be refused due to dirty source
    let dirty_source_err = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        dirty_source_err.starts_with(code::BAD_PARAMS)
            && dirty_source_err.to_lowercase().contains("dirty"),
        "dirty source must be refused: {dirty_source_err}"
    );

    // Target dirt: make main repo dirty
    std::fs::remove_file(wt_path.join("uncommitted.txt")).unwrap(); // clean source
    std::fs::write(repo.path().join("README.md"), "dirty main repo\n").unwrap(); // dirty target

    let dirty_target_err = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        dirty_target_err.starts_with(code::BAD_PARAMS)
            && dirty_target_err.to_lowercase().contains("dirty"),
        "dirty target must be refused: {dirty_target_err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_non_recorded_target_checkout_refused() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Wrong target branch test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    // Complete task
    c.call(
        "task.report",
        json!({ "task_id": &task_id, "status": "completed", "summary": "Done" }),
    )
    .await
    .unwrap();

    // Switch main repo to a different branch
    repo.git(&["checkout", "-b", "other-branch"]);

    // Attempt finish merge: must be refused because current branch in repo is not "main"
    let finish_err = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        finish_err.starts_with(code::BAD_PARAMS)
            && (finish_err.contains("main")
                || finish_err.contains("checkout")
                || finish_err.contains("branch")),
        "finish into non-recorded checkout must be refused: {finish_err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_preexisting_branch_never_deleted() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    // Create branch in advance
    repo.create_branch("pre-existing-feature");

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "branch": "pre-existing-feature",
                "contract": {"objective": "Preexisting branch test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(start_res["task"]["preexisting_branch"], true);

    // Complete task
    c.call(
        "task.report",
        json!({ "task_id": &task_id, "status": "completed", "summary": "Done" }),
    )
    .await
    .unwrap();

    // Finish with delete_branch: true
    let finish_res = c
        .call(
            "task.finish",
            json!({
                "task_id": &task_id,
                "mode": "merge",
                "delete_branch": true
            }),
        )
        .await
        .unwrap();

    assert_eq!(finish_res["task"]["disposition"]["outcome"], "merged");

    // The preexisting branch MUST still exist
    let branch_out = repo.git(&["branch", "--list", "pre-existing-feature"]);
    let branch_str = String::from_utf8_lossy(&branch_out.stdout);
    assert!(
        branch_str.contains("pre-existing-feature"),
        "pre-existing branch must never be deleted even if delete_branch: true"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_discard_scoping_and_disposition() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Discard test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());
    let pane_id = start_res["pane"]["id"].as_str().unwrap().to_string();

    assert!(wt_path.exists());

    // Discard from working state
    let discard_res = c
        .call(
            "task.finish",
            json!({
                "task_id": &task_id,
                "mode": "discard",
                "delete_branch": true,
            }),
        )
        .await
        .unwrap();

    assert_eq!(discard_res["task"]["disposition"]["outcome"], "discarded");
    assert_eq!(discard_res["task"]["state"], "canceled");

    // Worktree removed
    assert!(!wt_path.exists(), "discard must remove worktree path");

    // Repo itself remains intact
    assert!(repo.path().exists(), "main repo must not be touched");

    // The task workspace pointed at the removed worktree: it closes with
    // its worker pane.
    let ws_id = start_res["pane"]["workspace_id"].as_str().unwrap();
    let err = c
        .call("workspace.get", json!({ "workspace_id": ws_id }))
        .await
        .unwrap_err();
    assert!(err.starts_with(code::NO_SUCH_WORKSPACE), "{err}");
    let err = c
        .call("pane.get", json!({ "pane_id": pane_id }))
        .await
        .unwrap_err();
    assert!(err.starts_with(code::NO_SUCH_PANE), "{err}");

    srv.shutdown().await;
}

#[tokio::test]
async fn task_finish_preserves_workspace_only_for_live_outside_panes() {
    for live in [true, false] {
        let srv = TestServer::start().await;
        let mut c = srv.client().await;
        let repo = TempGitRepo::new();
        let wt = srv.state_dir.join("task-worktree");
        // A shared string prefix does not put this directory inside the worktree.
        let outside = srv.state_dir.join("task-worktree-other");
        std::fs::create_dir_all(&outside).unwrap();
        let start = c
            .call(
                "task.start",
                json!({
                    "repo": repo.path().to_string_lossy(),
                    "path": wt.to_string_lossy(),
                    "contract": {"objective": "Preserve unrelated work"},
                    "agent": "codex",
                    "argv": ["sleep", "60"],
                }),
            )
            .await
            .unwrap();
        let ws_id = &start["pane"]["workspace_id"];
        let tab = c
            .call(
                "tab.create",
                json!({"workspace_id": ws_id, "title": "Other work"}),
            )
            .await
            .unwrap();
        let pane = c
            .call(
                "pane.spawn",
                json!({
                    "workspace_id": ws_id,
                    "tab_id": tab["tab"]["id"],
                    "cwd": outside.to_string_lossy(),
                    "argv": if live { vec!["sleep", "60"] } else { vec!["true"] },
                }),
            )
            .await
            .unwrap();
        let pane_id = &pane["pane"]["id"];
        if !live {
            let wait = c
                .call(
                    "wait",
                    json!({"pane_id": pane_id, "until": "exited", "timeout_s": 5}),
                )
                .await
                .unwrap();
            assert_eq!(wait["satisfied"], true);
        }

        let finish = c
            .call(
                "task.finish",
                json!({"task_id": start["task"]["id"], "mode": "discard"}),
            )
            .await
            .unwrap();
        assert_eq!(finish["task"]["disposition"]["outcome"], "discarded");
        assert!(finish.get("cleanup_error").is_none());
        assert!(!wt.exists());
        assert!(outside.exists());

        let workspace = c
            .call("workspace.get", json!({"workspace_id": ws_id}))
            .await;
        let pane = c.call("pane.get", json!({"pane_id": pane_id})).await;
        if live {
            assert!(
                workspace.is_ok(),
                "live outside pane lost its workspace: {workspace:?}"
            );
            let pane = pane.unwrap();
            assert_eq!(pane["pane"]["live"]["state"], "live");
        } else {
            assert!(workspace.unwrap_err().starts_with(code::NO_SUCH_WORKSPACE));
            assert!(pane.unwrap_err().starts_with(code::NO_SUCH_PANE));
        }
        srv.shutdown().await;
    }
}

#[tokio::test]
async fn test_task_cancel_preserves_worktree_and_closes_worker_pane() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Cancel test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());
    let pane_id = start_res["pane"]["id"].as_str().unwrap().to_string();

    assert!(wt_path.exists());

    // Cancel task
    let cancel_res = c
        .call("task.cancel", json!({ "task_id": &task_id }))
        .await
        .unwrap();

    assert_eq!(cancel_res["task"]["state"], "canceled");

    // Worker pane closed
    let pane_res = c
        .call("pane.get", json!({ "pane_id": pane_id }))
        .await
        .unwrap();
    assert_eq!(pane_res["pane"]["live"]["state"], "exited");

    // Worktree and branch ARE PRESERVED for post-mortem inspection
    assert!(
        wt_path.exists(),
        "task.cancel must preserve worktree checkout"
    );

    // Second cancel on terminal task returns BAD_PARAMS
    let second_cancel = c
        .call("task.cancel", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        second_cancel.starts_with(code::BAD_PARAMS),
        "second cancel must return BAD_PARAMS: {second_cancel}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_failed_dirt_check_is_io_error_and_merges_nothing() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Dirt check failure test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let branch = start_res["task"]["branch"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());

    // Commit one change on the task branch, then break `git status` in the
    // worktree by removing its .git pointer file (worktrees keep .git as a
    // file). Uncommitted content stays behind to prove the point.
    std::fs::write(wt_path.join("file1.txt"), "committed\n").unwrap();
    let _ = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "add", "file1.txt"])
        .output();
    let _ = std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "commit", "-m", "commit 1"])
        .output();
    std::fs::write(wt_path.join("uncommitted.txt"), "dirt\n").unwrap();

    c.call(
        "task.report",
        json!({ "task_id": &task_id, "status": "completed", "summary": "Done" }),
    )
    .await
    .unwrap();

    std::fs::remove_file(wt_path.join(".git")).unwrap();
    assert!(
        !std::process::Command::new("git")
            .args(["-C", &wt_path.to_string_lossy(), "status", "--porcelain"])
            .output()
            .unwrap()
            .status
            .success(),
        "precondition: git status in the worktree fails"
    );

    // A failed dirt check is IO_ERROR, never a clean-tree merge or delete.
    let err = c
        .call(
            "task.finish",
            json!({ "task_id": &task_id, "mode": "merge" }),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::IO_ERROR),
        "failed dirt check must be IO_ERROR, got: {err}"
    );

    // Nothing merged, nothing recorded, worktree (with its dirt) intact.
    let get = c
        .call("task.get", json!({"task_id": &task_id}))
        .await
        .unwrap();
    assert_eq!(get["task"]["disposition"]["outcome"], "none");
    assert!(wt_path.join("uncommitted.txt").exists());
    let log = repo.git(&["log", "--oneline", "main"]);
    assert!(
        !String::from_utf8_lossy(&log.stdout).contains("commit 1"),
        "task branch must not have merged"
    );
    assert!(
        !repo.git(&["branch", "--list", &branch]).stdout.is_empty(),
        "task branch must survive the refused finish"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_finish_subscriber_sees_task_id() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Event task id"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();

    let mut sub = srv.client().await;
    sub.call(
        "subscribe",
        json!({"events": ["task.updated"], "task_ids": [&task_id]}),
    )
    .await
    .unwrap();

    c.call(
        "task.finish",
        json!({"task_id": &task_id, "mode": "discard"}),
    )
    .await
    .unwrap();

    // A task_ids filter keeps an event only when payload.task_id is set.
    // The finish record is that event; earlier updates (cancel) may precede it.
    let mut payload = None;
    for _ in 0..4 {
        let events = sub.read_events(1, Duration::from_secs(2)).await;
        if events[0]["payload"]["task"]["disposition"]["outcome"] == "discarded" {
            payload = Some(events[0]["payload"].clone());
            break;
        }
    }
    let payload = payload.expect("subscriber dropped the finish event");
    assert_eq!(
        payload["task_id"].as_str(),
        Some(task_id.as_str()),
        "task.updated must carry a top-level task_id, got {payload}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_second_finish_retries_cleanup_while_worktree_remains() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Cleanup retry"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());

    std::fs::write(wt_path.join("feature.txt"), "x\n").unwrap();
    assert!(std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "add", "feature.txt"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new("git")
        .args(["-C", &wt_path.to_string_lossy(), "commit", "-m", "feature"])
        .status()
        .unwrap()
        .success());
    c.call(
        "task.report",
        json!({"task_id": &task_id, "status": "completed", "summary": "done"}),
    )
    .await
    .unwrap();
    let first = c
        .call("task.finish", json!({"task_id": &task_id, "mode": "merge"}))
        .await
        .unwrap();
    assert_eq!(first["task"]["disposition"]["outcome"], "merged");
    assert!(!wt_path.exists(), "first finish removes the worktree");

    // A recorded disposition with the checkout path still on disk must retry
    // cleanup instead of refusing the second finish.
    std::fs::create_dir_all(&wt_path).unwrap();
    std::fs::write(wt_path.join("leftover.txt"), "still here\n").unwrap();
    let second = c
        .call("task.finish", json!({"task_id": &task_id, "mode": "merge"}))
        .await
        .unwrap();
    assert_eq!(second["task"]["disposition"]["outcome"], "merged");
    assert!(
        !wt_path.exists(),
        "second finish must remove the leftover worktree"
    );
    assert!(second.get("cleanup_error").is_none());

    srv.shutdown().await;
}

#[tokio::test]
async fn test_finish_uses_canonical_worktree_boundary() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "prefix boundary"},
                "agent": "codex",
                "argv": ["sleep", "60"],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let wt = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());
    let sibling = wt.with_file_name(format!(
        "{}-other",
        wt.file_name().unwrap().to_string_lossy()
    ));
    std::fs::create_dir_all(&sibling).unwrap();
    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": sibling.to_string_lossy(), "name": "sibling"}),
        )
        .await
        .unwrap();
    c.call(
        "pane.spawn",
        json!({
            "workspace_id": ws["workspace"]["id"],
            "cwd": sibling.to_string_lossy(),
            "argv": ["sleep", "60"],
        }),
    )
    .await
    .unwrap();

    let discarded = c
        .call(
            "task.finish",
            json!({"task_id": &task_id, "mode": "discard"}),
        )
        .await;
    assert!(
        discarded.is_ok(),
        "sibling {} must not count as inside {}: {discarded:?}",
        sibling.display(),
        wt.display()
    );

    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "symlink boundary"},
                "agent": "codex",
                "argv": ["sleep", "60"],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let wt = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());
    let link = std::env::temp_dir().join(format!(
        "signaltty-wt-link-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::os::unix::fs::symlink(&wt, &link).unwrap();
    let ws = c
        .call(
            "workspace.create",
            json!({"cwd": link.to_string_lossy(), "name": "via-link"}),
        )
        .await
        .unwrap();
    let pane = c
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws["workspace"]["id"],
                "cwd": link.to_string_lossy(),
                "argv": ["sleep", "60"],
            }),
        )
        .await
        .unwrap();
    let err = c
        .call(
            "task.finish",
            json!({"task_id": &task_id, "mode": "discard"}),
        )
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::PANES_ALIVE),
        "symlink cwd must block finish, got {err}"
    );
    c.call("pane.close", json!({"pane_id": pane["pane"]["id"]}))
        .await
        .unwrap();
    c.call(
        "task.finish",
        json!({"task_id": &task_id, "mode": "discard"}),
    )
    .await
    .unwrap();
    let _ = std::fs::remove_file(&link);

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_merge_is_bounded_and_leaves_target_clean() {
    let srv = TestServer::start_with_env(&[("SIGNALTTY_MERGE_TIMEOUT_MS", "500")]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Stalled merge"},
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());
    std::fs::write(wt_path.join("feature.txt"), "feature\n").unwrap();
    for args in [
        &["add", "feature.txt"][..],
        &["commit", "-m", "feature"][..],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    c.call(
        "task.report",
        json!({"task_id": &task_id, "status": "completed", "summary": "done"}),
    )
    .await
    .unwrap();

    // A stalled merge hook in the target repo.
    let hook = repo.path().join(".git/hooks/pre-merge-commit");
    let marker = repo.path().join(".git/hook-started");
    std::fs::write(
        &hook,
        format!("#!/bin/sh\ntouch '{}'\nsleep 30\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let started = std::time::Instant::now();
    let err = c
        .call("task.finish", json!({"task_id": &task_id, "mode": "merge"}))
        .await
        .unwrap_err();
    assert!(err.starts_with(code::TIMEOUT), "{err}");
    assert!(started.elapsed() < Duration::from_secs(15));
    assert!(marker.exists(), "the merge hook must have started");
    assert!(
        !repo.path().join(".git/MERGE_HEAD").exists(),
        "merge aborted"
    );
    assert!(
        !repo.path().join("feature.txt").exists(),
        "merge left its files behind"
    );
    let status = repo.git(&["status", "--porcelain", "--untracked-files=no"]);
    assert!(status.stdout.is_empty(), "target must stay clean");
    assert!(wt_path.exists(), "no cleanup after a timed-out merge");
    let task = c
        .call("task.get", json!({"task_id": &task_id}))
        .await
        .unwrap();
    assert_eq!(task["task"]["disposition"]["outcome"], "none");
    srv.shutdown().await;
}

fn pid_alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

#[tokio::test]
async fn test_task_finish_merge_reaps_the_worker_process_group_first() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    let pidfile = repo.path().join(".git/worker-child.pid");
    // Ignores HUP/TERM so only a group KILL stops it.
    let script = format!(
        "trap '' HUP TERM; sleep 300 & echo $! > '{}'; wait",
        pidfile.display()
    );
    let start = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Reap"},
                "argv": ["sh", "-c", script],
            }),
        )
        .await
        .unwrap();
    let task_id = start["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start["task"]["worktree_path"].as_str().unwrap());
    std::fs::write(wt_path.join("feature.txt"), "feature\n").unwrap();
    for args in [
        &["add", "feature.txt"][..],
        &["commit", "-m", "feature"][..],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    c.call(
        "task.report",
        json!({"task_id": &task_id, "status": "completed", "summary": "done"}),
    )
    .await
    .unwrap();
    let child_pid: i32 = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&pidfile) {
                if let Ok(pid) = text.trim().parse() {
                    return pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("worker child must start");
    assert!(pid_alive(child_pid));

    c.call("task.finish", json!({"task_id": &task_id, "mode": "merge"}))
        .await
        .unwrap();
    assert!(
        !pid_alive(child_pid),
        "worker descendants must be reaped before task.finish returns"
    );
    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_non_conflict_merge_failure_reports_git_error() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Hook refuses the merge"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());
    std::fs::write(wt_path.join("feature.txt"), "feature\n").unwrap();
    for args in [
        &["add", "feature.txt"][..],
        &["commit", "-m", "feature"][..],
    ] {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&wt_path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    c.call(
        "task.report",
        json!({"task_id": &task_id, "status": "completed", "summary": "done"}),
    )
    .await
    .unwrap();

    // The merge itself is clean; a pre-merge-commit hook refuses it.
    let hook = repo.path().join(".git/hooks/pre-merge-commit");
    std::fs::write(&hook, "#!/bin/sh\necho refused-by-hook >&2\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();

    let resp = c
        .call_raw_resp("task.finish", json!({"task_id": &task_id, "mode": "merge"}))
        .await
        .unwrap();
    let err = resp.error.unwrap();
    assert_eq!(err.code, code::IO_ERROR, "{}", err.message);
    assert!(err.message.contains("refused-by-hook"), "{}", err.message);
    assert_eq!(err.details["abort_ok"], true);

    srv.shutdown().await;
}
