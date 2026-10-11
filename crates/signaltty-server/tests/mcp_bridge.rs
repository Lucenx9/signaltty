use serde_json::{json, Value};
use signaltty_testkit::{bin_path, TestServer};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Bridge {
    child: tokio::process::Child,
    input: tokio::process::ChildStdin,
    output: BufReader<tokio::process::ChildStdout>,
}

impl Bridge {
    fn start(socket: &std::path::Path, pane: &str) -> Self {
        let mut child = tokio::process::Command::new(bin_path("signaltty"))
            .args(["mcp", "serve"])
            .env("SIGNALTTY_SOCKET", socket)
            .env("SIGNALTTY_PANE", pane)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.input
            .write_all(
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.output.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        serde_json::from_str(&line).expect("MCP stdout must contain protocol JSON")
    }

    async fn tool(&mut self, method: &str, params: Value) -> Value {
        self.call(
            "tools/call",
            json!({"name":"signaltty_call","arguments":{"method":method,"params":params}}),
        )
        .await
    }

    async fn initialize(&mut self) {
        let init = self.call("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})).await;
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    }

    async fn stop(mut self) {
        drop(self.input);
        assert!(self.child.wait().await.unwrap().success());
    }
}

#[tokio::test]
async fn bridge_rejects_missing_context_and_never_contacts_a_default_socket() {
    let mut bridge = Bridge::start(std::path::Path::new(""), "");
    bridge.initialize().await;
    let result = bridge.tool("server.status", json!({})).await;
    assert_eq!(result["result"]["isError"], true);
    assert!(result["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("No default server was contacted"));
    bridge.stop().await;
}

#[tokio::test]
async fn bridge_returns_errors_and_rejects_streaming_without_corrupting_later_calls() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id":w["workspace"]["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    let mut bridge = Bridge::start(&srv.socket, p["pane"]["id"].as_str().unwrap());
    assert_eq!(
        bridge.call("tools/list", json!({})).await["error"]["code"],
        -32000
    );
    bridge.initialize().await;
    let advertised = bridge.call("tools/list", json!({})).await;
    let methods = &advertised["result"]["tools"][1]["inputSchema"]["properties"]["method"]["enum"];
    assert!(!methods
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "decision.answer"));
    bridge
        .input
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\nnot json\n")
        .await
        .unwrap();
    let mut line = String::new();
    bridge.output.read_line(&mut line).await.unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["error"]["code"],
        -32700
    );
    for method in [
        "subscribe",
        "pane.attach",
        "server.shutdown",
        "hook-event",
        "report-session",
        "decision.answer",
    ] {
        let denied = bridge.tool(method, json!({})).await;
        assert_eq!(denied["result"]["isError"], true);
        assert!(
            denied["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("supported non-streaming"),
            "{denied}"
        );
    }
    let bad_params = bridge.tool("pane.get", json!({"pane_id":false})).await;
    assert!(bad_params["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("BAD_PARAMS"));
    assert_eq!(
        bridge.tool("server.status", json!({})).await["result"]["isError"],
        false
    );
    bridge.stop().await;
    let mut stale = Bridge::start(&srv.socket, "missing-pane");
    stale.initialize().await;
    assert_eq!(
        stale.tool("server.status", json!({})).await["result"]["isError"],
        true
    );
    stale.stop().await;
    srv.shutdown().await;
}

#[tokio::test]
async fn bridge_refuses_a_relative_socket_even_when_it_resolves_to_a_live_server() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id":w["workspace"]["id"],"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    let mut child = tokio::process::Command::new(bin_path("signaltty"))
        .args(["mcp", "serve"])
        .current_dir(srv.socket.parent().unwrap())
        .env("SIGNALTTY_SOCKET", srv.socket.file_name().unwrap())
        .env("SIGNALTTY_PANE", p["pane"]["id"].as_str().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut bridge = Bridge {
        input: child.stdin.take().unwrap(),
        output: BufReader::new(child.stdout.take().unwrap()),
        child,
    };
    bridge.initialize().await;
    let result = bridge.tool("server.status", json!({})).await;
    assert_eq!(result["result"]["isError"], true, "{result}");
    assert!(result["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("absolute SIGNALTTY_SOCKET"));
    bridge.stop().await;
    srv.shutdown().await;
}

#[tokio::test]
async fn bridge_closes_oversized_input_instead_of_buffering_unbounded_data() {
    let mut bridge = Bridge::start(std::path::Path::new(""), "");
    bridge
        .input
        .write_all(&vec![b' '; 1024 * 1024 + 1])
        .await
        .unwrap();
    drop(bridge.input);
    let status = tokio::time::timeout(std::time::Duration::from_secs(5), bridge.child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn bridge_inspects_the_host_and_launches_a_worker_through_real_ipc() {
    let srv = TestServer::start().await;
    let mut c = srv.client().await;
    let w = c
        .call("workspace.create", json!({"cwd":"/tmp"}))
        .await
        .unwrap();
    let ws = w["workspace"]["id"].as_str().unwrap();
    let p = c
        .call(
            "pane.spawn",
            json!({"workspace_id":ws,"argv":["sleep","30"]}),
        )
        .await
        .unwrap();
    let pane = p["pane"]["id"].as_str().unwrap();
    let mut bridge = Bridge::start(&srv.socket, pane);
    let init = bridge.call("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})).await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    let tools = bridge.call("tools/list", json!({})).await;
    assert!(tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["name"] == "signaltty_call"));
    let context = bridge
        .call(
            "tools/call",
            json!({"name":"signaltty_context","arguments":{}}),
        )
        .await;
    assert_eq!(context["result"]["structuredContent"]["pane"]["id"], pane);
    let worker = bridge
        .tool(
            "pane.split",
            json!({"pane_id":pane,"direction":"right","argv":["sh","-c","printf 'worker reached host' ; sleep 30"]}),
        )
        .await;
    assert_eq!(worker["result"]["isError"], false, "{worker}");
    let worker_id = worker["result"]["structuredContent"]["pane"]["id"]
        .as_str()
        .unwrap();
    let output = bridge
        .tool(
            "pane.wait_for_output",
            json!({"pane_id":worker_id,"match":"worker reached host","timeout_s":5}),
        )
        .await;
    assert_eq!(output["result"]["isError"], false);
    assert!(output["result"]["structuredContent"]["text"]
        .as_str()
        .unwrap()
        .contains("worker reached host"));
    bridge.stop().await;
    srv.shutdown().await;
}
