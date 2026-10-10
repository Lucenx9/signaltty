//! Latest-turn changes (spec 041): baselines captured at turn start.

use std::time::Duration;

use serde_json::{json, Value};
use signaltty_testkit::{FakeAgentPane, TempGitRepo, TestClient, TestServer};

async fn agent_workspace(client: &mut TestClient, repo: &TempGitRepo) -> (Value, FakeAgentPane) {
    let workspace = client
        .call(
            "workspace.create",
            json!({"cwd": repo.path(), "name": "turns"}),
        )
        .await
        .unwrap()["workspace"]
        .clone();
    let pane = client
        .call(
            "pane.spawn",
            json!({"workspace_id": workspace["id"], "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (workspace, FakeAgentPane::new(pane, "codex"))
}

async fn turn_diff(client: &mut TestClient, workspace: &Value) -> Value {
    client
        .call(
            "workspace.diff",
            json!({"workspace_id": workspace["id"], "scope": "turn"}),
        )
        .await
        .unwrap()
}

fn paths(diff: &Value) -> Vec<&str> {
    diff["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect()
}

fn write(repo: &TempGitRepo, path: &str, text: &str) {
    std::fs::write(repo.path().join(path), text).unwrap();
}

fn git(repo: &TempGitRepo, args: &[&str]) {
    let out = repo.git(args);
    assert!(out.status.success(), "{args:?}: {:?}", out.stderr);
}

/// Start a turn and wait until its baseline is recorded.
async fn start_turn(server: &TestServer, client: &mut TestClient, agent: &FakeAgentPane) -> Value {
    let mut watcher = server.client().await;
    watcher
        .call("subscribe", json!({"events": ["workspace.turn_started"]}))
        .await
        .unwrap();
    agent.prompt_submit(client).await.unwrap();
    watcher.read_events(1, Duration::from_secs(10)).await[0]["payload"].clone()
}

#[tokio::test]
async fn turn_scope_lists_committed_and_uncommitted_work_but_not_earlier_dirt() {
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let repo = TempGitRepo::new();
    // Dirt from before the turn: an edited tracked file and an untracked one.
    write(&repo, "README.md", "initial\nedited before\n");
    write(&repo, "notes.txt", "before\n");
    let (workspace, agent) = agent_workspace(&mut client, &repo).await;

    let before = turn_diff(&mut client, &workspace).await;
    assert_eq!(before["scope"], "turn");
    assert!(before["turn"].is_null(), "{before}");
    assert!(paths(&before).is_empty());

    let started = start_turn(&server, &mut client, &agent).await;
    assert_eq!(started["workspace_id"], workspace["id"]);
    assert_eq!(started["pane_id"], agent.pane_id.as_str());

    // The turn commits one file and leaves another edit uncommitted.
    write(&repo, "new.rs", "fn main() {}\n");
    git(&repo, &["add", "new.rs"]);
    git(&repo, &["commit", "-qm", "agent work"]);
    write(&repo, "notes.txt", "before\nafter\n");

    let turn = turn_diff(&mut client, &workspace).await;
    assert_eq!(turn["turn"]["pane_id"], agent.pane_id.as_str());
    assert_eq!(paths(&turn), ["new.rs", "notes.txt"]);
    assert_eq!(turn["added"], 2);
    assert_eq!(turn["removed"], 0);

    // HEAD scope is unchanged: the commit hides new.rs, earlier dirt shows.
    let head = client
        .call("workspace.diff", json!({"workspace_id": workspace["id"]}))
        .await
        .unwrap();
    assert_eq!(head["scope"], "head");
    assert_eq!(paths(&head), ["README.md", "notes.txt"]);

    let notes = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id": workspace["id"], "path": "notes.txt", "scope": "turn"}),
        )
        .await
        .unwrap();
    let lines: Vec<_> = notes["content"]["hunks"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["kind"].as_str().unwrap(), l["text"].as_str().unwrap()))
        .collect();
    assert_eq!(lines, [("context", "before"), ("added", "after")]);
    let readme = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id": workspace["id"], "path": "README.md", "scope": "turn"}),
        )
        .await
        .unwrap();
    assert_eq!(readme["content"]["kind"], "unchanged", "{readme}");
}

#[tokio::test]
async fn a_permission_prompt_does_not_start_a_new_turn() {
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let repo = TempGitRepo::new();
    let (workspace, agent) = agent_workspace(&mut client, &repo).await;
    let started = start_turn(&server, &mut client, &agent).await;
    write(&repo, "first.txt", "one\n");

    agent
        .permission_request(&mut client, "Bash", json!({"command": "ls"}))
        .await
        .unwrap();
    // blocked → working continues the same turn.
    agent.prompt_submit(&mut client).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    let turn = turn_diff(&mut client, &workspace).await;
    assert_eq!(turn["turn"]["started_at"], started["started_at"]);
    assert_eq!(paths(&turn), ["first.txt"]);
}

#[tokio::test]
async fn turn_file_read_without_a_turn_is_bad_params() {
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let repo = TempGitRepo::new();
    let (workspace, _agent) = agent_workspace(&mut client, &repo).await;
    let err = client
        .call(
            "workspace.file_diff",
            json!({"workspace_id": workspace["id"], "path": "README.md", "scope": "turn"}),
        )
        .await
        .unwrap_err();
    assert!(err.contains("BAD_PARAMS"), "{err}");
}
