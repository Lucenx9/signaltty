//! Interactive attach: raw-mode terminal streaming `pty.data`,
//! stdin forwarded as `pane.input`. Detach key: Ctrl+].

use base64::Engine;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use crate::client::{CliError, Client};

const DETACH_KEY: u8 = 0x1d; // Ctrl+]

pub async fn run(socket: &std::path::Path, pane_id: &str) -> Result<(), CliError> {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut client = Client::connect(socket).await?;
    // Attach streams only `pty.data`; subscribe first so an exit racing
    // the attach still arrives.
    client
        .call("subscribe", json!({"events": ["pane.exited"]}))
        .await?;
    let attach = client
        .call(
            "pane.attach",
            json!({"pane_id": pane_id, "cols": cols, "rows": rows}),
        )
        .await?;
    if let Some(snap) = attach.get("snapshot_b64").and_then(|v| v.as_str()) {
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(snap) {
            let mut out = tokio::io::stdout();
            out.write_all(&bytes)
                .await
                .map_err(|e| CliError::Io(e.to_string()))?;
            out.flush().await.map_err(|e| CliError::Io(e.to_string()))?;
        }
    }
    if attach["live"]["state"] != "live" {
        return Ok(());
    }

    crossterm::terminal::enable_raw_mode().map_err(|e| CliError::Io(e.to_string()))?;
    let result = attach_loop(socket, pane_id, client).await;
    let _ = crossterm::terminal::disable_raw_mode();
    println!();
    result
}

async fn attach_loop(
    socket: &std::path::Path,
    pane_id: &str,
    client: Client,
) -> Result<(), CliError> {
    let (reader, _writer) = client.into_parts();
    // `next_line` is cancel-safe; `read_line` would drop a partly received
    // event whenever stdin wins the select below.
    let mut lines = reader.lines();
    let mut input_client = Client::connect(socket).await?;
    let mut stdin = tokio::io::stdin();
    let mut winch = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::window_change())
        .map_err(|e| CliError::Io(e.to_string()))?;
    let mut out = tokio::io::stdout();
    let mut in_buf = [0u8; 4096];

    loop {
        tokio::select! {
            n = stdin.read(&mut in_buf) => {
                let n = n.map_err(|e| CliError::Io(e.to_string()))?;
                if n == 0 {
                    return Ok(());
                }
                let chunk = &in_buf[..n];
                if let Some(pos) = chunk.iter().position(|&b| b == DETACH_KEY) {
                    // Forward bytes before the detach key, then detach.
                    if pos > 0 {
                        send_input(&mut input_client, pane_id, &chunk[..pos]).await?;
                    }
                    return Ok(());
                }
                send_input(&mut input_client, pane_id, chunk).await?;
            }
            line = lines.next_line() => {
                let Some(line) = line.map_err(|e| CliError::Io(e.to_string()))? else {
                    return Err(CliError::Io("server closed connection".to_string()));
                };
                let v: Value = serde_json::from_str(line.trim()).map_err(|e| CliError::Io(e.to_string()))?;
                if v.get("type").and_then(|t| t.as_str()) != Some("event") {
                    continue;
                }
                let name = v.get("event").and_then(|e| e.as_str()).unwrap_or("");
                let payload = v.get("payload").cloned().unwrap_or(Value::Null);
                match name {
                    "pty.data" => {
                        if let Some(b64) = payload.get("data_b64").and_then(|v| v.as_str()) {
                            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                                out.write_all(&bytes).await.map_err(|e| CliError::Io(e.to_string()))?;
                                out.flush().await.map_err(|e| CliError::Io(e.to_string()))?;
                            }
                        }
                    }
                    "pane.exited" if payload["pane_id"] == pane_id => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
            _ = winch.recv() => {
                if let Ok((cols, rows)) = crossterm::terminal::size() {
                    let _ = input_client.call("pane.resize",
                        json!({"pane_id": pane_id, "cols": cols, "rows": rows})).await;
                }
            }
        }
    }
}

async fn send_input(client: &mut Client, pane_id: &str, data: &[u8]) -> Result<(), CliError> {
    client
        .call(
            "pane.input",
            json!({
                "pane_id": pane_id,
                "data_b64": base64::engine::general_purpose::STANDARD.encode(data),
            }),
        )
        .await?;
    Ok(())
}
