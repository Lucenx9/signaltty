use serde_json::json;
use signaltty_testkit::TestServer;

#[tokio::test]
async fn automatic_naming_is_opt_in_and_manual_rename_disables_it() {
    let server = TestServer::start().await;
    let mut client = server.client().await;
    let manual = client
        .call("workspace.create", json!({"name":"manual"}))
        .await
        .unwrap();
    assert_eq!(manual["workspace"]["naming"], "manual");
    let automatic = client
        .call(
            "workspace.create",
            json!({"name":"project", "auto_name":true}),
        )
        .await
        .unwrap();
    assert_eq!(automatic["workspace"]["naming"], "awaiting_prompt");
    let id = automatic["workspace"]["id"].clone();
    let handle = automatic["workspace"]["handle"].clone();
    let renamed = client
        .call(
            "workspace.rename",
            json!({"workspace_id":id,"name":"My choice"}),
        )
        .await
        .unwrap();
    assert_eq!(renamed["workspace"]["naming"], "manual");
    assert_eq!(renamed["workspace"]["handle"], handle);
    assert!(
        !client
            .call_raw_resp("workspace.create", json!({"auto_name":"yes"}))
            .await
            .unwrap()
            .ok
    );
    server.shutdown().await;
}

#[tokio::test]
async fn first_prompt_generates_once_without_blocking_or_changing_the_handle() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "signaltty-title-fake-{}",
        signaltty_core::new_ws_id()
    ));
    std::fs::create_dir(&dir).unwrap();
    let binary = dir.join("codex");
    std::fs::write(&binary, "#!/bin/sh\nif [ \"$1\" = mcp ]; then printf '[]'; exit 0; fi\ncat >/dev/null\nwhile [ $# -gt 0 ]; do\nif [ \"$1\" = '--output-last-message' ]; then shift; output=$1; fi\nshift\ndone\nsleep 0.1\nprintf '%s' '{\"title\":\"Fix keyboard navigation\"}' > \"$output\"\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut server =
        TestServer::start_with_env(&[("SIGNALTTY_TITLE_CODEX_BIN", binary.to_str().unwrap())])
            .await;
    let mut client = server.client().await;
    let ws = client
        .call(
            "workspace.create",
            json!({"name":"project","auto_name":true}),
        )
        .await
        .unwrap()["workspace"]
        .clone();
    let pane = client
        .call(
            "pane.spawn",
            json!({"workspace_id":ws["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .clone();
    let started = std::time::Instant::now();
    client.call("hook-event",json!({"agent":"codex","event":"UserPromptSubmit","pane_id":pane,"payload":{"prompt":"Please fix keyboard navigation"}})).await.unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let result = tokio::time::timeout(std::time::Duration::from_secs(4), async {
        loop {
            let result = client
                .call("workspace.get", json!({"workspace_id":ws["id"]}))
                .await
                .unwrap();
            if result["workspace"]["naming"] == "generated" {
                break result;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("first prompt must generate a title");
    assert_eq!(result["workspace"]["name"], "Fix keyboard navigation");
    assert_eq!(result["workspace"]["handle"], ws["handle"]);
    client.call("hook-event",json!({"agent":"codex","event":"UserPromptSubmit","pane_id":pane,"payload":{"prompt":"A later unrelated prompt"}})).await.unwrap();
    assert_eq!(
        client
            .call("workspace.get", json!({"workspace_id":ws["id"]}))
            .await
            .unwrap()["workspace"]["name"],
        "Fix keyboard navigation"
    );
    let race = client
        .call("workspace.create", json!({"name":"race","auto_name":true}))
        .await
        .unwrap()["workspace"]
        .clone();
    let race_pane = client
        .call(
            "pane.spawn",
            json!({"workspace_id":race["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .clone();
    client.call("hook-event",json!({"agent":"codex","event":"UserPromptSubmit","pane_id":race_pane,"payload":{"prompt":"Choose a title"}})).await.unwrap();
    client
        .call(
            "workspace.rename",
            json!({"workspace_id":race["id"],"name":"Keep my name"}),
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let result = client
        .call("workspace.get", json!({"workspace_id":race["id"]}))
        .await
        .unwrap();
    assert_eq!(result["workspace"]["name"], "Keep my name");
    assert_eq!(result["workspace"]["naming"], "manual");

    // Invalid model output retains the provisional name and consumes the attempt.
    std::fs::write(
        &binary,
        "#!/bin/sh\nif [ \"$1\" = mcp ]; then printf '[]'; exit 0; fi\ncat >/dev/null\nexit 1\n",
    )
    .unwrap();
    let failure = client
        .call(
            "workspace.create",
            json!({"name":"fallback","auto_name":true}),
        )
        .await
        .unwrap()["workspace"]
        .clone();
    let failed_pane = client
        .call(
            "pane.spawn",
            json!({"workspace_id":failure["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap()["pane"]["id"]
        .clone();
    client.call("hook-event",json!({"agent":"codex","event":"UserPromptSubmit","pane_id":failed_pane,"payload":{"prompt":"Choose a title"}})).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let result = client
                .call("workspace.get", json!({"workspace_id":failure["id"]}))
                .await
                .unwrap();
            if result["workspace"]["naming"] == "failed" {
                assert_eq!(result["workspace"]["name"], "fallback");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    server.restart().await;
    let mut client = server.client().await;
    let restored = client
        .call("workspace.get", json!({"workspace_id":ws["id"]}))
        .await
        .unwrap();
    assert_eq!(restored["workspace"]["name"], "Fix keyboard navigation");
    assert_eq!(restored["workspace"]["naming"], "generated");
    assert_eq!(
        client
            .call("workspace.get", json!({"workspace_id":failure["id"]}))
            .await
            .unwrap()["workspace"]["naming"],
        "failed"
    );
    server.shutdown().await;
    std::fs::remove_dir_all(dir).unwrap();
}
