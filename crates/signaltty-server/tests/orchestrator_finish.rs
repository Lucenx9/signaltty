use std::path::PathBuf;

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

    // Call task.finish with merge
    let finish_res = c
        .call(
            "task.finish",
            json!({
                "task_id": &task_id,
                "mode": "merge",
                "delete_branch": true,
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

    // Worker pane closed
    let pane_res = c
        .call("pane.get", json!({ "pane_id": pane_id }))
        .await
        .unwrap();
    assert_eq!(pane_res["pane"]["live"]["state"], "exited");

    srv.shutdown().await;
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
