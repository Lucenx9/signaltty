use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::json;
use signaltty_testkit::{TempGitRepo, TestClient, TestServer};

fn agent_fixture(directory: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(directory).unwrap();
    let path = directory.join(name);
    std::fs::write(&path, "#!/bin/sh\nif [ \"$*\" = '--no-daemon --version' ]; then exit 0; fi\nprintf 'native-arg:%s\\n' \"$@\"\nfor arg in \"$@\"; do if [ \"$arg\" = '--version' ]; then exit 0; fi; done\nwhile read line; do :; done\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

async fn wait_for_arg(client: &mut TestClient, pane: &str, argument: &str) {
    let marker = format!("native-arg:{argument}");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let result = client
            .call(
                "pane.read",
                json!({"pane_id":pane, "mode":"tail", "lines":50}),
            )
            .await
            .unwrap();
        if result["text"].as_str().unwrap_or("").contains(&marker) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "missing process argument: {marker}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn automatic_codex_resume_runs_with_recorded_permissions() {
    let mut server = TestServer::start().await;
    let codex = agent_fixture(server.socket.parent().unwrap(), "codex");
    let mut client = server.client().await;
    let ws = client
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let spawn = client.call("pane.spawn", json!({"workspace_id":ws["workspace"]["id"], "argv":[codex, "--dangerously-bypass-approvals-and-sandbox", "--version"]})).await.unwrap();
    let pane = spawn["pane"]["id"].as_str().unwrap();
    client
        .call(
            "wait",
            json!({"pane_id":pane, "until":"exited", "timeout_s":15}),
        )
        .await
        .unwrap();
    let report = client
        .call(
            "report-session",
            json!({"pane_id":pane, "agent":"codex", "agent_session_id":"automatic-session"}),
        )
        .await
        .unwrap();
    let expected = json!([
        codex,
        "--dangerously-bypass-approvals-and-sandbox",
        "resume",
        "automatic-session"
    ]);
    assert_eq!(report["pane"]["agent"]["resume_argv"], expected);
    // Restore from a snapshot as on the other PC after an application restart.
    server.restart().await;
    let mut client = server.client().await;
    let resumed = client
        .call("pane.resume", json!({"pane_id":pane}))
        .await
        .unwrap();
    assert_eq!(resumed["pane"]["agent"]["resume_argv"], expected);
    wait_for_arg(
        &mut client,
        pane,
        "--dangerously-bypass-approvals-and-sandbox",
    )
    .await;
    wait_for_arg(&mut client, pane, "automatic-session").await;
    server.shutdown().await;
}

#[tokio::test]
async fn default_worker_inherits_automatic_mode_but_explicit_and_other_providers_do_not() {
    let repo = TempGitRepo::new();
    let bin = repo.path().join("fixture-bin");
    let codex = agent_fixture(&bin, "codex");
    agent_fixture(&bin, "claude");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let server = TestServer::start_with_env(&[("PATH", &path)]).await;
    let mut client = server.client().await;
    let ws = client
        .call("workspace.create", json!({"cwd":repo.path()}))
        .await
        .unwrap();
    let parent = client.call("pane.spawn", json!({"workspace_id":ws["workspace"]["id"], "argv":[codex, "--dangerously-bypass-approvals-and-sandbox"]})).await.unwrap();
    let parent = parent["pane"]["id"].as_str().unwrap();
    for (agent, explicit, parent_id, expected) in [
        (
            "codex",
            None,
            Some(parent),
            json!(["codex", "--dangerously-bypass-approvals-and-sandbox"]),
        ),
        (
            "codex",
            Some(json!(["codex"])),
            Some(parent),
            json!(["codex"]),
        ),
        ("claude", None, Some(parent), json!(["claude"])),
        ("codex", None, None, json!(["codex"])),
    ] {
        let mut params = json!({"repo":repo.path(), "agent":agent, "parent_pane_id":parent_id, "contract":{"objective":"Check automatic launch policy"}});
        if let Some(argv) = explicit {
            params["argv"] = argv;
        }
        let start = client.call("task.start", params).await.unwrap();
        assert_eq!(start["pane"]["argv"], expected);
        assert_eq!(start["task"]["worker_cmd"], expected);
        if expected.as_array().unwrap().len() > 1 {
            wait_for_arg(
                &mut client,
                start["pane"]["id"].as_str().unwrap(),
                "--dangerously-bypass-approvals-and-sandbox",
            )
            .await;
        }
        client
            .call("task.cancel", json!({"task_id":start["task"]["id"]}))
            .await
            .unwrap();
    }
    server.shutdown().await;
}

#[tokio::test]
async fn antigravity_workers_inherit_the_automatic_parent_choice() {
    let repo = TempGitRepo::new();
    let bin = repo.path().join("fixture-bin");
    let codex = agent_fixture(&bin, "codex");
    for name in ["agy", "antigravity", "antigravity-cli"] {
        agent_fixture(&bin, name);
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let server = TestServer::start_with_env(&[("PATH", &path)]).await;
    let mut client = server.client().await;
    let ws = client
        .call("workspace.create", json!({"cwd": repo.path()}))
        .await
        .unwrap();
    let parent = client
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws["workspace"]["id"],
                "argv": [codex, "--dangerously-bypass-approvals-and-sandbox"],
            }),
        )
        .await
        .unwrap();
    let parent_id = parent["pane"]["id"].as_str().unwrap();
    for (agent, explicit, parent, expected) in [
        (
            "agy",
            None,
            Some(parent_id),
            json!([
                "agy",
                "--dangerously-skip-permissions",
                "--mode",
                "accept-edits"
            ]),
        ),
        (
            "antigravity",
            None,
            Some(parent_id),
            json!([
                "antigravity",
                "--dangerously-skip-permissions",
                "--mode",
                "accept-edits"
            ]),
        ),
        (
            "antigravity-cli",
            None,
            Some(parent_id),
            json!([
                "antigravity-cli",
                "--dangerously-skip-permissions",
                "--mode",
                "accept-edits"
            ]),
        ),
        (
            "agy",
            Some(json!([bin.join("agy")])),
            Some(parent_id),
            json!([
                bin.join("agy"),
                "--dangerously-skip-permissions",
                "--mode",
                "accept-edits"
            ]),
        ),
        (
            "agy",
            Some(json!(["agy", "--mode", "plan"])),
            Some(parent_id),
            json!(["agy", "--mode", "plan"]),
        ),
        ("agy", None, None, json!(["agy"])),
    ] {
        let mut params = json!({"repo":repo.path(), "agent":agent, "parent_pane_id":parent, "contract":{"objective":"Review without unnecessary confirmations"}});
        if let Some(argv) = explicit {
            params["argv"] = argv;
        }
        let start = client.call("task.start", params).await.unwrap();
        assert_eq!(start["pane"]["argv"], expected);
        assert_eq!(start["task"]["worker_cmd"], expected);
        assert_eq!(start["pane"]["agent"]["kind"], "generic");
        if expected
            .as_array()
            .unwrap()
            .iter()
            .any(|arg| arg == "--dangerously-skip-permissions")
        {
            wait_for_arg(
                &mut client,
                start["pane"]["id"].as_str().unwrap(),
                "--dangerously-skip-permissions",
            )
            .await;
        }
        client
            .call("task.cancel", json!({"task_id":start["task"]["id"]}))
            .await
            .unwrap();
    }
    server.shutdown().await;
}
