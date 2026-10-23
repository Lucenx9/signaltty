//! A relative `--socket` is resolved against the server's start directory:
//! panes and plugin hooks inherit `SIGNALTTY_SOCKET` but run elsewhere.

use std::process::Stdio;

use serde_json::json;
use signaltty_testkit::{bin_path, TestClient};

struct Server(std::process::Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn relative_socket_reaches_panes_as_an_absolute_path() {
    let base = std::path::PathBuf::from(format!("/tmp/st-relsock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let elsewhere = base.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let home = base.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let _server = Server(
        std::process::Command::new(bin_path("signaltty-server"))
            .current_dir(&base)
            .args(["--socket", "s.sock", "--state-dir", "state"])
            .env("SIGNALTTY_INTEGRATION_HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("HOME", &home)
            .env("SIGNALTTY_LOGIND", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn signaltty-server"),
    );
    let socket = base.join("s.sock");
    let mut client = None;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        if let Ok(mut c) = TestClient::connect(&socket).await {
            if c.call("server.status", json!({})).await.is_ok() {
                client = Some(c);
                break;
            }
        }
    }
    let mut c = client.expect("server listening on the relative socket");
    let ws = c
        .call("workspace.create", json!({"cwd": elsewhere}))
        .await
        .unwrap();
    let pane = c
        .call(
            "pane.spawn",
            json!({
                "workspace_id": ws["workspace"]["id"],
                "argv": ["sh", "-c", "echo \"SOCK=$SIGNALTTY_SOCKET\"; sleep 30"],
            }),
        )
        .await
        .unwrap();
    let read = c
        .call(
            "pane.wait_for_output",
            json!({"pane_id": pane["pane"]["id"], "match": "SOCK=", "timeout_s": 10}),
        )
        .await
        .unwrap();
    assert_eq!(
        read["matched_line"].as_str().unwrap().trim(),
        format!("SOCK={}", socket.display())
    );
    let _ = std::fs::remove_dir_all(&base);
}
