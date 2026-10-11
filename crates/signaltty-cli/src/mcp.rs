//! Harness-owned stdio transport; shell sandbox policy remains unchanged.
use crate::client::{CliError, Client};
use serde_json::{json, Value};
use signaltty_proto::method;
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const MAX_INPUT: usize = 1024 * 1024;
const VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

fn allowed(method: &str) -> bool {
    method::ALL.contains(&method)
        && !matches!(
            method,
            method::SUBSCRIBE
                | method::PANE_ATTACH
                | method::PANE_DETACH
                | method::SERVER_SHUTDOWN
                | method::HOOK_EVENT
                | method::REPORT_SESSION
        )
}

fn tools() -> Value {
    json!({"tools":[
        {"name":"signaltty_context", "description":"Inspect the hosting Signaltty pane and the API schema. Use these tools when shell networking blocks the local socket.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"openWorldHint":false}},
        {"name":"signaltty_call", "description":"Call the hosting Signaltty workspace/PTY/orchestration API. Can start workers (including agy) in server-owned panes. params follows signaltty_context's schema and Signaltty docs; streaming attach/subscribe and server shutdown are unavailable. The harness's normal tool approvals apply.",
         "inputSchema":{"type":"object","properties":{
            "method":{"type":"string","enum":method::ALL.iter().filter(|m|allowed(m)).collect::<Vec<_>>()},
            "params":{"type":"object"}},"required":["method"],"additionalProperties":false},
         "annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false,"openWorldHint":true}}
    ]})
}

fn context() -> Result<(PathBuf, String), CliError> {
    let socket = std::env::var_os("SIGNALTTY_SOCKET")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| CliError::Usage("Signaltty MCP requires an absolute SIGNALTTY_SOCKET from the hosting pane; launch the harness inside Signaltty. No default server was contacted.".into()))?;
    let pane = std::env::var("SIGNALTTY_PANE")
        .ok()
        .filter(|p| !p.is_empty())
        .ok_or_else(|| CliError::Usage("Signaltty MCP requires SIGNALTTY_PANE from the hosting pane; restart the harness in that pane.".into()))?;
    Ok((socket, pane))
}

async fn execute(params: &Value) -> Result<Value, CliError> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| CliError::Usage("tools/call requires a tool name".into()))?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !args.is_object() {
        return Err(CliError::Usage("tool arguments must be an object".into()));
    }
    if !matches!(name, "signaltty_context" | "signaltty_call") {
        return Err(CliError::Usage(format!("unknown tool '{name}'")));
    }
    let (socket, pane_id) = context()?;
    let mut client = Client::connect(&socket).await?;
    let pane = client
        .call(method::PANE_GET, json!({"pane_id":pane_id}))
        .await?;
    if name == "signaltty_context" {
        let schema = client.call(method::SERVER_SCHEMA, json!({})).await?;
        return Ok(json!({"pane":pane["pane"],"schema":schema}));
    }
    let method = args["method"]
        .as_str()
        .filter(|m| allowed(m))
        .ok_or_else(|| {
            CliError::Usage("method must name a supported non-streaming Signaltty API".into())
        })?;
    let params = args.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err(CliError::Usage("Signaltty params must be an object".into()));
    }
    client.call(method, params).await
}

fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

async fn respond(request: Value, initialized: &mut bool) -> Option<Value> {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if !request.is_object()
        || request["jsonrpc"] != "2.0"
        || !id.is_null() && !id.is_string() && !id.is_number()
        || !request["method"].is_string()
    {
        return Some(error(Value::Null, -32600, "invalid JSON-RPC request"));
    }
    // Notifications never execute tools or emit responses.
    if request.get("id").is_none() {
        return None;
    }
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Some(error(id, -32602, "params must be an object"));
    }
    let result = match request["method"].as_str().unwrap() {
        "initialize" => {
            let Some(version) = params["protocolVersion"].as_str() else {
                return Some(error(id, -32602, "initialize requires protocolVersion"));
            };
            *initialized = true;
            json!({"protocolVersion":if VERSIONS.contains(&version){version}else{VERSIONS.last().unwrap()},
                "capabilities":{"tools":{}},
                "serverInfo":{"name":"signaltty","version":env!("CARGO_PKG_VERSION")},
                "instructions":"Use Signaltty MCP for workspace and pane operations when the shell cannot connect to local sockets. signaltty_context identifies the hosting pane. Tool approvals remain controlled by your harness."})
        }
        "ping" => json!({}),
        "tools/list" if *initialized => tools(),
        "tools/call" if *initialized => {
            match tokio::time::timeout(std::time::Duration::from_secs(60), execute(&params)).await {
                Ok(Ok(value)) => {
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                }
                result => {
                    let message = match result {
                        Ok(Err(e)) => e.to_string(),
                        Err(_) => "Signaltty tool timed out after 60 seconds; an operation may already have been applied. Inspect state before retrying.".into(),
                        _ => unreachable!(),
                    };
                    json!({"content":[{"type":"text","text":message}],"isError":true})
                }
            }
        }
        "tools/list" | "tools/call" => return Some(error(id, -32000, "initialize first")),
        _ => return Some(error(id, -32601, "method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

pub async fn serve() -> Result<(), CliError> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::stdout();
    let mut initialized = false;
    loop {
        let mut line = Vec::new();
        let n = (&mut reader)
            .take((MAX_INPUT + 1) as u64)
            .read_until(b'\n', &mut line)
            .await
            .map_err(|e| CliError::Io(e.to_string()))?;
        if n == 0 {
            return Ok(());
        }
        if n > MAX_INPUT {
            return Err(CliError::Usage(
                "MCP input exceeds 1 MiB; closing stream".into(),
            ));
        }
        let response = match serde_json::from_slice(&line) {
            Ok(request) => respond(request, &mut initialized).await,
            Err(_) => Some(error(Value::Null, -32700, "invalid JSON")),
        };
        if let Some(response) = response {
            writer
                .write_all(format!("{response}\n").as_bytes())
                .await
                .map_err(|e| CliError::Io(e.to_string()))?;
            writer
                .flush()
                .await
                .map_err(|e| CliError::Io(e.to_string()))?;
        }
    }
}
