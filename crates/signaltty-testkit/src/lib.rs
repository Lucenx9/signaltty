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
    /// Re-applied by `restart()` so overrides such as PATH survive it.
    extra_envs: Vec<(String, String)>,
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

    pub async fn start_with_env(envs: &[(&str, &str)]) -> TestServer {
        Self::start_internal(None, None, envs).await
    }

    /// Start with explicit plugin + agents dirs (None = server default).
    pub async fn start_with_dirs(
        plugin_dir: Option<&Path>,
        agents_dir: Option<&Path>,
    ) -> TestServer {
        Self::start_internal(plugin_dir, agents_dir, &[]).await
    }

    async fn start_internal(
        plugin_dir: Option<&Path>,
        agents_dir: Option<&Path>,
        extra_envs: &[(&str, &str)],
    ) -> TestServer {
        let ns = format!("st-{}", unique());
        let temp_dir = std::env::temp_dir();
        // Keep unix domain socket path under SUN_LEN (108 bytes).
        let parent = if temp_dir.as_os_str().len() > 35 {
            PathBuf::from("/tmp")
        } else {
            temp_dir
        };
        let base = parent.join(ns);
        let socket = base.join("s.sock");
        let state_dir = base.join("state");
        let integration_home = base.join("home");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&integration_home).unwrap();
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
            .env("XDG_CONFIG_HOME", integration_home.join(".config"))
            .env("XDG_DATA_HOME", integration_home.join(".local/share"))
            .env("HOME", &integration_home);
        for (k, v) in extra_envs {
            cmd.env(k, v);
        }
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
            extra_envs: extra_envs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
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
            .env("XDG_DATA_HOME", self.integration_home.join(".local/share"))
            .env("HOME", &self.integration_home)
            .envs(self.extra_envs.iter().map(|(k, v)| (k, v)))
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

/// Hermetic temporary git repository for tests.
pub struct TempGitRepo {
    path: PathBuf,
}

impl TempGitRepo {
    pub fn new() -> Self {
        Self::with_branch(None)
    }

    pub fn with_branch(branch: Option<&str>) -> Self {
        let ns = format!("st-repo-{}", unique());
        let temp_dir = std::env::temp_dir();
        let parent = if temp_dir.as_os_str().len() > 35 {
            PathBuf::from("/tmp")
        } else {
            temp_dir
        };
        let path = parent.join(ns);
        std::fs::create_dir_all(&path).unwrap();
        let repo = Self { path };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        let _ = repo.git(&["checkout", "-B", "main"]);
        std::fs::write(repo.path().join("README.md"), "initial\n").unwrap();
        repo.git(&["add", "."]);
        repo.git(&["commit", "-qm", "initial commit"]);
        if let Some(b) = branch {
            repo.create_branch(b);
        }
        repo
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn head_sha(&self) -> String {
        let out = self.git(&["rev-parse", "HEAD"]);
        assert!(
            out.status.success(),
            "rev-parse HEAD failed: {:?}",
            out.stderr
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    pub fn create_branch(&self, branch_name: &str) {
        let out = self.git(&["branch", branch_name]);
        assert!(
            out.status.success(),
            "create branch {branch_name} failed: {:?}",
            out.stderr
        );
    }

    pub fn git(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&self.path)
            .args(args)
            .output()
            .expect("execute git")
    }
}

impl Default for TempGitRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempGitRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Synthetic hook-event driver for a fake agent pane.
pub struct FakeAgentPane {
    pub pane_id: String,
    pub agent: String,
    pub session_id: String,
}

impl FakeAgentPane {
    pub fn new(pane_id: impl Into<String>, agent: impl Into<String>) -> Self {
        let agent = agent.into();
        let session_id = format!("{}-sess-{}", agent, unique());
        Self {
            pane_id: pane_id.into(),
            agent,
            session_id,
        }
    }

    pub async fn session_start(
        &self,
        client: &mut TestClient,
        cwd: &Path,
    ) -> Result<Value, String> {
        client
            .call(
                "hook-event",
                json!({
                    "agent": self.agent,
                    "event": "SessionStart",
                    "pane_id": self.pane_id,
                    "payload": {
                        "session_id": self.session_id,
                        "cwd": cwd.to_string_lossy(),
                    }
                }),
            )
            .await
    }

    pub async fn prompt_submit(&self, client: &mut TestClient) -> Result<Value, String> {
        client
            .call(
                "hook-event",
                json!({
                    "agent": self.agent,
                    "event": "UserPromptSubmit",
                    "pane_id": self.pane_id,
                    "payload": {
                        "session_id": self.session_id,
                    }
                }),
            )
            .await
    }

    pub async fn permission_request(
        &self,
        client: &mut TestClient,
        tool_name: &str,
        tool_input: Value,
    ) -> Result<Value, String> {
        client
            .call(
                "hook-event",
                json!({
                    "agent": self.agent,
                    "event": "PermissionRequest",
                    "pane_id": self.pane_id,
                    "payload": {
                        "session_id": self.session_id,
                        "tool_name": tool_name,
                        "tool_input": tool_input,
                    }
                }),
            )
            .await
    }

    pub async fn stop(&self, client: &mut TestClient) -> Result<Value, String> {
        client
            .call(
                "hook-event",
                json!({
                    "agent": self.agent,
                    "event": "Stop",
                    "pane_id": self.pane_id,
                    "payload": {
                        "session_id": self.session_id,
                    }
                }),
            )
            .await
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
        let resp = self.call_raw_resp(method, params).await?;
        if resp.ok {
            return Ok(resp.result);
        }
        let e = resp.error.unwrap();
        Err(format!("{}: {}", e.code, e.message))
    }

    /// Single call returning the raw `Response` envelope (to inspect error details).
    pub async fn call_raw_resp(&mut self, method: &str, params: Value) -> Result<Response, String> {
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
            return Ok(resp);
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
