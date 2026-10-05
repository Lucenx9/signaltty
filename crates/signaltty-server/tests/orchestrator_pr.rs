//! Integration tests for task pull-request cycle (spec 020).
//! Uses a fake `gh` script on PATH and a local bare git repo as origin.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::json;
use signaltty_proto::code;
use signaltty_testkit::{TempGitRepo, TestServer};

fn unique_temp_dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("st-pr-{}", signaltty_core::ids::new_task_id()));
    fs::create_dir_all(&p).unwrap();
    p
}

struct TestEnv {
    fake_bin: PathBuf,
    gh_log: PathBuf,
    gh_fixture: PathBuf,
    _gh_counter: PathBuf,
    gh_fail_url: PathBuf,
    bare_repo: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let base = unique_temp_dir();
        let fake_bin = base.join("bin");
        fs::create_dir_all(&fake_bin).unwrap();

        let gh_log = base.join("gh.log");
        let gh_fixture = base.join("gh_fixture.json");
        let _gh_counter = base.join("gh_counter.txt");
        let gh_fail_url = base.join("gh_fail_url.txt");
        let bare_repo = base.join("bare.git");

        // Init bare git repo
        let out = std::process::Command::new("git")
            .args(["init", "--bare", bare_repo.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(out.status.success(), "init bare repo failed");

        // Write fake gh script
        let gh_script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
if [ "$FAKE_GH_FAIL" = "1" ]; then
    echo "simulated gh failure" >&2
    exit 1
fi
if [ "$1" = "pr" ] && [ "$2" = "create" ]; then
    COUNT=7
    if [ -f "{counter}" ]; then
        COUNT=$(cat "{counter}")
        COUNT=$((COUNT + 1))
    fi
    echo "$COUNT" > "{counter}"
    echo "https://github.com/o/r/pull/$COUNT"
    exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    if [ -f "{fail_url}" ] && [ "$3" = "$(cat "{fail_url}")" ]; then
        echo "simulated pr view failure for $3" >&2
        exit 1
    fi
    if [ -f "{fixture}" ]; then
        cat "{fixture}"
    else
        echo '{{"state":"OPEN","reviewDecision":"APPROVED","statusCheckRollup":[]}}'
    fi
    exit 0
fi
echo "unknown fake gh invocation: $@" >&2
exit 2
"#,
            log = gh_log.display(),
            fixture = gh_fixture.display(),
            counter = _gh_counter.display(),
            fail_url = gh_fail_url.display(),
        );

        let gh_path = fake_bin.join("gh");
        fs::write(&gh_path, gh_script).unwrap();
        let mut perms = fs::metadata(&gh_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&gh_path, perms).unwrap();

        Self {
            fake_bin,
            gh_log,
            gh_fixture,
            _gh_counter,
            gh_fail_url,
            bare_repo,
        }
    }

    fn attach_origin(&self, repo_path: &Path) {
        let out = std::process::Command::new("git")
            .args([
                "-C",
                repo_path.to_str().unwrap(),
                "remote",
                "add",
                "origin",
                self.bare_repo.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git remote add origin failed: {:?}",
            out.stderr
        );
    }
}

#[tokio::test]
async fn test_task_pr_open_happy_path_and_git_push() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Implement PR feature"},
                "label": "My PR Feature",
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();
    let branch = start_res["task"]["branch"].as_str().unwrap().to_string();
    let wt_path = PathBuf::from(start_res["task"]["worktree_path"].as_str().unwrap());

    // Commit a change in the task worktree
    fs::write(wt_path.join("feature.txt"), "hello world\n").unwrap();
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

    // Report completed
    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Feature done",
        }),
    )
    .await
    .unwrap();

    // Call task.pr_open
    let pr_res = c
        .call(
            "task.pr_open",
            json!({
                "task_id": &task_id,
            }),
        )
        .await
        .unwrap();

    let pr = &pr_res["task"]["pr"];
    assert_eq!(pr["number"], 7);
    assert_eq!(pr["url"], "https://github.com/o/r/pull/7");
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["checks"], "none");
    assert_eq!(pr["review"], "none");

    // Verify branch was pushed to bare repo origin
    let out = std::process::Command::new("git")
        .args([
            "-C",
            env.bare_repo.to_str().unwrap(),
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}", branch),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "branch {} was not pushed to bare origin",
        branch
    );

    // Verify gh was invoked with expected arguments
    let gh_log_contents = fs::read_to_string(&env.gh_log).unwrap();
    assert!(gh_log_contents.contains("pr create"));
    assert!(gh_log_contents.contains(&format!("--head {}", branch)));
    assert!(gh_log_contents.contains("--base main"));
    assert!(gh_log_contents.contains("--title My PR Feature"));

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_pr_open_refused_when_not_completed_or_has_pr() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "PR refusal test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    // 1. Refused when not completed
    let err = c
        .call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::BAD_PARAMS),
        "expected BAD_PARAMS, got: {err}"
    );

    // Complete task
    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done",
        }),
    )
    .await
    .unwrap();

    // 2. Open PR succeeds
    let ok = c.call("task.pr_open", json!({ "task_id": &task_id })).await;
    assert!(ok.is_ok(), "pr_open should succeed after completed");

    // 3. Refused when task already has a PR
    let err2 = c
        .call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        err2.starts_with(code::BAD_PARAMS),
        "expected BAD_PARAMS for second pr_open, got: {err2}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_finish_merge_refused_while_pr_open() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "PR merge refusal"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done",
        }),
    )
    .await
    .unwrap();

    c.call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap();

    // Finish mode merge must be refused with BAD_PARAMS while PR is open
    let finish_err = c
        .call(
            "task.finish",
            json!({
                "task_id": &task_id,
                "mode": "merge",
            }),
        )
        .await
        .unwrap_err();
    assert!(
        finish_err.starts_with(code::BAD_PARAMS),
        "expected BAD_PARAMS, got: {finish_err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_pr_refresh_mapping() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    // Explicit task_id with no PR -> BAD_PARAMS
    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Refresh test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    let refresh_err = c
        .call("task.pr_refresh", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        refresh_err.starts_with(code::BAD_PARAMS),
        "expected BAD_PARAMS, got: {refresh_err}"
    );

    // Complete and open PR
    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done",
        }),
    )
    .await
    .unwrap();
    c.call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap();

    // 1. Refresh with failing checks and changes_requested
    fs::write(
        &env.gh_fixture,
        json!({
            "state": "OPEN",
            "reviewDecision": "CHANGES_REQUESTED",
            "statusCheckRollup": [
                {"status": "COMPLETED", "conclusion": "FAILURE"}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let ref_res = c
        .call("task.pr_refresh", json!({ "task_id": &task_id }))
        .await
        .unwrap();
    let tasks = ref_res["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    let pr = &tasks[0]["pr"];
    assert_eq!(pr["state"], "open");
    assert_eq!(pr["checks"], "failing");
    assert_eq!(pr["review"], "changes_requested");
    assert!(pr["checked_at"].is_string());

    // 2. Refresh with passing checks and approved
    fs::write(
        &env.gh_fixture,
        json!({
            "state": "OPEN",
            "reviewDecision": "APPROVED",
            "statusCheckRollup": [
                {"status": "COMPLETED", "conclusion": "SUCCESS"}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let ref_res2 = c.call("task.pr_refresh", json!({})).await.unwrap();
    let tasks2 = ref_res2["tasks"].as_array().unwrap();
    assert_eq!(tasks2.len(), 1);
    let pr2 = &tasks2[0]["pr"];
    assert_eq!(pr2["checks"], "passing");
    assert_eq!(pr2["review"], "approved");

    // 3. Refresh with state MERGED
    fs::write(
        &env.gh_fixture,
        json!({
            "state": "MERGED",
            "reviewDecision": "APPROVED",
            "statusCheckRollup": [
                {"status": "COMPLETED", "conclusion": "SUCCESS"}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let ref_res3 = c
        .call("task.pr_refresh", json!({ "task_id": &task_id }))
        .await
        .unwrap();
    let tasks3 = ref_res3["tasks"].as_array().unwrap();
    assert_eq!(tasks3.len(), 1);
    assert_eq!(tasks3[0]["pr"]["state"], "merged");

    // Now that PR is merged, calling pr_refresh with no task_id returns empty array
    let ref_res4 = c.call("task.pr_refresh", json!({})).await.unwrap();
    assert_eq!(ref_res4["tasks"].as_array().unwrap().len(), 0);

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_pr_open_missing_gh_spawn_failed() {
    let env = TestEnv::new();
    let bin_dir = unique_temp_dir();
    // Symlink git and sh so task.start and git push work, but gh is not present
    let git_out = std::process::Command::new("which")
        .arg("git")
        .output()
        .unwrap();
    let git_path = String::from_utf8(git_out.stdout)
        .unwrap()
        .trim()
        .to_string();
    std::os::unix::fs::symlink(&git_path, bin_dir.join("git")).unwrap();
    let sh_out = std::process::Command::new("which")
        .arg("sh")
        .output()
        .unwrap();
    let sh_path = String::from_utf8(sh_out.stdout).unwrap().trim().to_string();
    std::os::unix::fs::symlink(&sh_path, bin_dir.join("sh")).unwrap();
    let path_no_gh = bin_dir.to_string_lossy().to_string();

    let srv = TestServer::start_with_env(&[("PATH", &path_no_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Missing gh test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done",
        }),
    )
    .await
    .unwrap();

    let err = c
        .call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::SPAWN_FAILED),
        "expected SPAWN_FAILED, got: {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_pr_open_gh_error_io_error_with_details() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh), ("FAKE_GH_FAIL", "1")]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    let start_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Failing gh test"},
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();

    let task_id = start_res["task"]["id"].as_str().unwrap().to_string();

    c.call(
        "task.report",
        json!({
            "task_id": &task_id,
            "status": "completed",
            "summary": "Done",
        }),
    )
    .await
    .unwrap();

    let err = c
        .call("task.pr_open", json!({ "task_id": &task_id }))
        .await
        .unwrap_err();
    assert!(
        err.starts_with(code::IO_ERROR),
        "expected IO_ERROR, got: {err}"
    );
    assert!(
        err.contains("simulated gh failure"),
        "expected stderr in error message, got: {err}"
    );

    srv.shutdown().await;
}

#[tokio::test]
async fn test_task_pr_refresh_all_skips_failing_pr() {
    let env = TestEnv::new();
    let original_path = std::env::var("PATH").unwrap();
    let path_with_gh = format!("{}:{}", env.fake_bin.display(), original_path);

    let srv = TestServer::start_with_env(&[("PATH", &path_with_gh)]).await;
    let mut c = srv.client().await;
    let repo = TempGitRepo::new();
    env.attach_origin(repo.path());

    // Task 1
    let t1_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Task 1 objective"},
                "label": "Task 1",
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task1_id = t1_res["task"]["id"].as_str().unwrap().to_string();

    c.call(
        "task.report",
        json!({
            "task_id": &task1_id,
            "status": "completed",
            "summary": "Task 1 done",
        }),
    )
    .await
    .unwrap();

    let pr1_res = c
        .call("task.pr_open", json!({ "task_id": &task1_id }))
        .await
        .unwrap();
    let pr1_url = pr1_res["task"]["pr"]["url"].as_str().unwrap().to_string();
    assert_eq!(pr1_url, "https://github.com/o/r/pull/7");

    // Task 2
    let t2_res = c
        .call(
            "task.start",
            json!({
                "repo": repo.path().to_string_lossy(),
                "contract": {"objective": "Task 2 objective"},
                "label": "Task 2",
                "agent": "codex",
                "argv": ["sh"],
            }),
        )
        .await
        .unwrap();
    let task2_id = t2_res["task"]["id"].as_str().unwrap().to_string();

    c.call(
        "task.report",
        json!({
            "task_id": &task2_id,
            "status": "completed",
            "summary": "Task 2 done",
        }),
    )
    .await
    .unwrap();

    let pr2_res = c
        .call("task.pr_open", json!({ "task_id": &task2_id }))
        .await
        .unwrap();
    let pr2_url = pr2_res["task"]["pr"]["url"].as_str().unwrap().to_string();
    assert_eq!(pr2_url, "https://github.com/o/r/pull/8");

    // Make fake gh fail pr view for task 1's URL, succeed for task 2
    fs::write(&env.gh_fail_url, &pr1_url).unwrap();

    // Calling pr_refresh with no task_id should skip failing task 1 and return only task 2
    let ref_all_res = c.call("task.pr_refresh", json!({})).await.unwrap();
    let refreshed_tasks = ref_all_res["tasks"].as_array().unwrap();
    assert_eq!(
        refreshed_tasks.len(),
        1,
        "refresh-all should return only the good task"
    );
    assert_eq!(refreshed_tasks[0]["id"], task2_id);
    assert_eq!(refreshed_tasks[0]["pr"]["url"], pr2_url);
    assert!(refreshed_tasks[0]["pr"]["checked_at"].is_string());

    // Explicit task_id for the failing PR still returns the error
    let fail_err = c
        .call("task.pr_refresh", json!({ "task_id": &task1_id }))
        .await
        .unwrap_err();
    assert!(
        fail_err.starts_with(code::IO_ERROR),
        "expected IO_ERROR for explicit failing task, got: {fail_err}"
    );

    srv.shutdown().await;
}
