use serde_json::json;
use signaltty_testkit::{FakeAgentPane, TempGitRepo, TestServer};

#[test]
fn temp_git_repo_initial_commit_and_branch() {
    let repo = TempGitRepo::new();
    assert!(repo.path().exists());
    let head = repo.head_sha();
    assert_eq!(head.len(), 40);

    repo.create_branch("feature-orch");
    let out = repo.git(&["branch", "--list", "feature-orch"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("feature-orch"));
}

#[tokio::test]
async fn synthetic_hook_driver_and_server_restart() {
    let mut srv = TestServer::start().await;
    let mut c = srv.client().await;

    let ws = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let ws_id = ws["workspace"]["id"].as_str().unwrap();

    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    let pane_id = p["pane"]["id"].as_str().unwrap();

    let driver = FakeAgentPane::new(pane_id, "codex");
    let r = driver
        .session_start(&mut c, std::path::Path::new("/tmp"))
        .await
        .unwrap();
    assert_eq!(r["lifecycle"], "idle");

    driver.prompt_submit(&mut c).await.unwrap();
    let p_info = c
        .call("pane.get", json!({"pane_id": pane_id}))
        .await
        .unwrap();
    assert_eq!(p_info["pane"]["lifecycle"], "working");

    // Test server restart on same state dir
    srv.restart().await;
    let mut c2 = srv.client().await;
    let st = c2.call("server.status", json!({})).await.unwrap();
    assert_eq!(st["workspaces"], 1);

    srv.shutdown().await;
}
