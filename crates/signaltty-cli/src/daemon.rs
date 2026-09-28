//! Ensure the server is running, starting it detached when needed.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::client::{CliError, Client};

fn server_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let next_to = dir.join("signaltty-server");
            if next_to.is_file() {
                return Some(next_to);
            }
        }
    }
    // Fall back to PATH lookup.
    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join("signaltty-server"))
                .find(|p| p.is_file())
        })
        .or_else(|| {
            // Development fallback: target/debug next to the workspace.
            if let Ok(exe) = std::env::current_exe() {
                for anc in exe.ancestors() {
                    let cand = anc.join("signaltty-server");
                    if cand.is_file() {
                        return Some(cand);
                    }
                    // current_exe = .../target/debug/signaltty → deps/…? step up.
                    let cand = anc.join("target/debug/signaltty-server");
                    if cand.is_file() {
                        return Some(cand);
                    }
                }
            }
            None
        })
}

pub async fn ensure_running(socket: &Path, json: bool) -> Result<(), CliError> {
    if let Ok(mut c) = Client::connect(socket).await {
        if let Ok(status) = c.call("server.status", serde_json::json!({})).await {
            if json {
                println!("{}", serde_json::to_string(&status).unwrap());
            } else {
                println!("server already running on {}", socket.display());
            }
            return Ok(());
        }
        // Stale socket; fall through and start.
    }
    let bin = server_binary()
        .ok_or_else(|| CliError::Usage("signaltty-server binary not found".to_string()))?;
    let log = signaltty_core::paths::state_dir().join("server.log");
    std::fs::create_dir_all(log.parent().unwrap()).ok();
    let log_file = std::fs::File::create(&log).map_err(|e| CliError::Io(e.to_string()))?;
    std::process::Command::new(&bin)
        .arg("--socket")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            log_file
                .try_clone()
                .map_err(|e| CliError::Io(e.to_string()))?,
        ))
        .stderr(Stdio::from(log_file))
        .spawn()
        .map_err(|e| CliError::Io(format!("failed to start {}: {e}", bin.display())))?;

    // Poll for readiness.
    for _ in 0..50 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Ok(mut c) = Client::connect(socket).await {
            if let Ok(status) = c.call("server.status", serde_json::json!({})).await {
                if json {
                    println!("{}", serde_json::to_string(&status).unwrap());
                } else {
                    println!(
                        "server started on {} (log: {})",
                        socket.display(),
                        log.display()
                    );
                }
                return Ok(());
            }
        }
    }
    Err(CliError::Io(
        "server did not become ready in time".to_string(),
    ))
}
