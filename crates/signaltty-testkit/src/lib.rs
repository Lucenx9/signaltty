//! Shared integration-test harness: hermetic server (temp socket +
//! state dir), JSONL client helpers. Uses real PTYs via the server.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use signaltty_proto::Response;

/// Locate a workspace binary (target/debug/<name>) from the test executable.
pub fn bin_path(name: &str) -> PathBuf {
    let exe = std::env::current_exe().expect("current_exe");
    // .../target/debug/deps/testbin → .../target/debug/<name>
    for anc in exe.ancestors() {
        let cand = anc.join(name);
        if cand.is_file() {
            return cand;
        }
        // Also handle being directly in target/debug.
        if anc
            .file_name()
            .map(|n| n == "debug" || n == "release")
            .unwrap_or(false)
        {
            let cand = anc.join(name);
            if cand.is_file() {
                return cand;
            }
        }
    }
    panic!("binary {name} not found (built? run cargo build/test first)");
}

pub struct TestServer {
    pub socket: PathBuf,
    pub state_dir: PathBuf,
    pub integration_home: PathBuf,
    child: std::process::Child,
}

impl TestServer {
    pub async fn start() -> TestServer {
        Self::start_with_plugin_dir(None).await
    }

    /// Start with an explicit plugin dir (None = server default).
    pub async fn start_with_plugin_dir(plugin_dir: Option<&Path>) -> TestServer {
        Self::start_with_dirs(plugin_dir, None).await
    }

    /// Start with explicit plugin + agents dirs (None = server default).
    pub async fn start_with_dirs(
        plugin_dir: Option<&Path>,
        agents_dir: Option<&Path>,
    ) -> TestServer {
        let ns = format!("signaltty-test-{}-{}", std::process::id(), unique());
        let base = std::env::temp_dir().join(ns);
        let socket = base.join("signaltty.sock");
        let state_dir = base.join("state");
        let integration_home = base.join("home");
        std::fs::create_dir_all(&base).unwrap();
        let bin = bin_path("signaltty-server");
        let mut cmd = std::process::Command::new(bin);
        cmd.arg("--socket")
            .arg(&socket)
            .arg("--state-dir")
            .arg(&state_dir);
        if let Some(dir) = plugin_dir {
            cmd.arg("--plugin-dir").arg(dir);
        }
        if let Some(dir) = agents_dir {
            cmd.arg("--agents-dir").arg(dir);
        }
        cmd.env("SIGNALTTY_INTEGRATION_HOME", &integration_home)
            .env("XDG_CONFIG_HOME", integration_home.join(".config"));
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn signaltty-server");
        // Wait for readiness.
        let mut ready = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(mut c) = TestClient::connect(&socket).await {
                if c.call("server.status", json!({})).await.is_ok() {
                    ready = true;
                    break;
                }
            }
            if let Ok(Some(_)) = child.try_wait() {
                panic!("server exited during startup");
            }
        }
        assert!(ready, "server did not become ready");
        TestServer {
            socket,
            state_dir,
            integration_home,
            child,
        }
    }

    pub async fn client(&self) -> TestClient {
        TestClient::connect(&self.socket).await.expect("connect")
    }

    /// Ask the server to shut down (force), then reap.
    pub async fn shutdown(mut self) {
        if let Ok(mut c) = TestClient::connect(&self.socket).await {
            let _ = c.call("server.shutdown", json!({"force": true})).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        std::fs::remove_dir_all(self.socket.parent().unwrap()).ok();
    }

    /// Kill -9 without graceful shutdown (crash simulation).
    pub async fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Graceful shutdown + respawn on the SAME socket/state paths,
    /// exercising snapshot restore.
    pub async fn restart(&mut self) {
        if let Ok(mut c) = TestClient::connect(&self.socket).await {
            let _ = c.call("server.shutdown", json!({"force": true})).await;
        }
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(Some(_)) = self.child.try_wait() {
                break;
            }
        }
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        let bin = bin_path("signaltty-server");
        self.child = std::process::Command::new(bin)
            .env("SIGNALTTY_INTEGRATION_HOME", &self.integration_home)
            .env("XDG_CONFIG_HOME", self.integration_home.join(".config"))
            .arg("--socket")
            .arg(&self.socket)
            .arg("--state-dir")
            .arg(&self.state_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("respawn signaltty-server");
        let mut ready = false;
        for _ in 0..100 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(mut c) = TestClient::connect(&self.socket).await {
                if c.call("server.status", json!({})).await.is_ok() {
                    ready = true;
                    break;
                }
            }
        }
        assert!(ready, "respawned server did not become ready");
    }
}

fn unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    // Mix in nanos for cross-process uniqueness.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as u64;
    N.fetch_add(1, Ordering::Relaxed) ^ nanos ^ (std::process::id() as u64) << 32
}

pub struct TestClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl TestClient {
    pub async fn connect(socket: &Path) -> std::io::Result<TestClient> {
        let stream = UnixStream::connect(socket).await?;
        let (r, w) = stream.into_split();
        Ok(TestClient {
            reader: BufReader::new(r),
            writer: w,
            next_id: 1,
        })
    }

    /// Single call; skips interleaved events.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let line = serde_json::json!({
            "protocol": signaltty_proto::PROTOCOL,
            "id": id,
            "method": method,
            "params": params,
        });
        let mut s = serde_json::to_string(&line).unwrap();
        s.push('\n');
        self.writer
            .write_all(s.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        loop {
            let mut buf = String::new();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("eof".to_string());
            }
            let v: Value = serde_json::from_str(buf.trim()).map_err(|e| e.to_string())?;
            if v.get("type").and_then(|t| t.as_str()) == Some("event") {
                continue;
            }
            let resp: Response = serde_json::from_value(v).map_err(|e| e.to_string())?;
            if resp.ok {
                return Ok(resp.result);
            }
            let e = resp.error.unwrap();
            return Err(format!("{}: {}", e.code, e.message));
        }
    }

    /// Send a raw line, read one raw line back (for malformed-input tests).
    pub async fn raw_roundtrip(&mut self, line: &str) -> String {
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.write_all(b"\n").await.unwrap();
        let mut buf = String::new();
        self.reader.read_line(&mut buf).await.unwrap();
        buf
    }

    /// Read `count` raw event lines (connection must be subscribed).
    pub async fn read_events(&mut self, count: usize, timeout: std::time::Duration) -> Vec<Value> {
        let mut out = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout;
        while out.len() < count {
            let mut buf = String::new();
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let n = tokio::time::timeout(remaining, self.reader.read_line(&mut buf))
                .await
                .expect("event timeout")
                .unwrap();
            assert!(n > 0, "eof while waiting for events");
            let v: Value = serde_json::from_str(buf.trim()).unwrap();
            if v.get("type").and_then(|t| t.as_str()) == Some("event") {
                out.push(v);
            }
        }
        out
    }

    /// Subscribe and collect `count` matching events (test helper).
    pub async fn subscribe_collect(
        &mut self,
        events: Value,
        count: usize,
        timeout: std::time::Duration,
    ) -> Vec<Value> {
        self.call("subscribe", json!({"events": events}))
            .await
            .unwrap();
        let mut out = Vec::new();
        let deadline = tokio::time::Instant::now() + timeout;
        while out.len() < count {
            let mut buf = String::new();
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let n = tokio::time::timeout(remaining, self.reader.read_line(&mut buf))
                .await
                .expect("event timeout")
                .unwrap();
            assert!(n > 0, "eof while waiting for events");
            let v: Value = serde_json::from_str(buf.trim()).unwrap();
            if v.get("type").and_then(|t| t.as_str()) == Some("event") {
                out.push(v);
            }
        }
        out
    }
}
