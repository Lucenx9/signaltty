//! Screen-detection rules (spec 027) over real PTYs: manifest `[[screen]]`
//! rules classify hook-less panes; hooks always dominate.

use serde_json::{json, Value};
use signaltty_testkit::{TestClient, TestServer};

const RULES: &str = r#"
[agent]
kind = "generic"

[[screen]]
id = "ask"
state = "blocked"
regex = ['Allow\? \[y/n\]']

[[screen]]
id = "busy"
state = "working"
lines = 1
regex = ['^working\.\.\.$']

[[screen]]
id = "prompt"
state = "idle"
lines = 1
regex = ['^ready>$']
"#;

async fn server_with(manifest_kind: &str) -> TestServer {
    let dir = std::env::temp_dir().join(format!(
        "signaltty-screen-{}-{}",
        std::process::id(),
        signaltty_core::ids::new_pane_id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let body = RULES.replace("kind = \"generic\"", &format!("kind = \"{manifest_kind}\""));
    std::fs::write(dir.join("rules.toml"), body).unwrap();
    TestServer::start_with_dirs(None, Some(&dir)).await
}

async fn spawn(c: &mut TestClient, script: &str) -> String {
    let w = c
        .call("workspace.create", json!({"cwd": "/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id": w["workspace"]["id"], "argv": ["sh", "-c", script]}),
        )
        .await
        .unwrap();
    p["pane"]["id"].as_str().unwrap().to_string()
}

async fn wait(c: &mut TestClient, pane: &str, until: &str, secs: u64) -> Result<Value, String> {
    c.call(
        "wait",
        json!({"pane_id": pane, "until": until, "timeout_s": secs}),
    )
    .await
}

async fn pane(c: &mut TestClient, pane: &str) -> Value {
    c.call("pane.get", json!({"pane_id": pane})).await.unwrap()["pane"].clone()
}

#[tokio::test]
async fn approval_prompt_on_screen_blocks_the_pane() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "input_required");
    srv.shutdown().await;
}

#[tokio::test]
async fn working_then_prompt_finishes_the_turn() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "echo working...; sleep 2; echo 'ready>'; sleep 30").await;
    wait(&mut c, &id, "working", 5).await.unwrap();
    wait(&mut c, &id, "done", 5).await.unwrap();
    assert_eq!(pane(&mut c, &id).await["attention"], "unread");
    srv.shutdown().await;
}

#[tokio::test]
async fn a_hook_silences_screen_rules_for_that_process() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "sleep 1; printf 'Allow? [y/n] '; sleep 30").await;
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "UserPromptSubmit", "pane_id": id}),
    )
    .await
    .unwrap();
    let lifecycle = pane(&mut c, &id).await["lifecycle"].clone();
    assert!(wait(&mut c, &id, "blocked", 3).await.is_err());
    assert_eq!(pane(&mut c, &id).await["lifecycle"], lifecycle);
    srv.shutdown().await;
}

#[tokio::test]
async fn rules_for_another_kind_leave_the_pane_alone() {
    let srv = server_with("codex").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    let before = pane(&mut c, &id).await;
    assert!(wait(&mut c, &id, "blocked", 2).await.is_err());
    let after = pane(&mut c, &id).await;
    assert_eq!(after["lifecycle"], before["lifecycle"]);
    assert_eq!(after["attention"], before["attention"]);
    srv.shutdown().await;
}

#[tokio::test]
async fn a_hook_takes_over_a_screen_blocked_pane() {
    let srv = server_with("generic").await;
    let mut c = srv.client().await;
    let id = spawn(&mut c, "printf 'Allow? [y/n] '; sleep 30").await;
    wait(&mut c, &id, "blocked", 5).await.unwrap();
    c.call(
        "hook-event",
        json!({"agent": "claude", "event": "Stop", "pane_id": id}),
    )
    .await
    .unwrap();
    let p = pane(&mut c, &id).await;
    assert_eq!(
        (p["lifecycle"].clone(), p["attention"].clone()),
        (json!("done"), json!("unread"))
    );
    srv.shutdown().await;
}
