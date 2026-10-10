//! A set-but-empty `HOME` or `SHELL` falls back like an unset one, as the
//! XDG and provider variables already do.

use serde_json::json;
use signaltty_testkit::TestServer;

#[tokio::test]
async fn empty_home_and_shell_fall_back_to_defaults() {
    let srv = TestServer::start_with_env(&[("HOME", ""), ("SHELL", "")]).await;
    let mut c = srv.client().await;
    let ws = c.call("workspace.create", json!({})).await.unwrap();
    assert_eq!(ws["workspace"]["cwd"], "/tmp");
    let ws_id = ws["workspace"]["id"].as_str().unwrap();
    let first = c
        .call(
            "pane.spawn",
            json!({"workspace_id": ws_id, "argv": ["sleep", "30"]}),
        )
        .await
        .unwrap();
    // No argv: the user's shell, which is `sh` when SHELL is empty.
    let split = c
        .call(
            "pane.split",
            json!({"pane_id": first["pane"]["id"], "direction": "right"}),
        )
        .await
        .unwrap();
    assert_eq!(split["pane"]["argv"], json!(["sh"]));
}
