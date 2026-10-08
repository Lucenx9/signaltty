//! Unix-socket listener, connection handling, event fan-out.

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};

use signaltty_proto::{code, EventMsg, Request, Response, MAX_LINE_BYTES};

use crate::config::Config;
use crate::pty::PtyManager;
use crate::router::{dispatch, Ctx};
use crate::store::{SharedStore, Store, StoredEvent};

pub async fn serve(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let runtime_dir = config
        .socket_path
        .parent()
        .ok_or("socket path has no parent")?
        .to_path_buf();
    // Only tighten permissions on directories we create; never chmod a
    // shared parent (e.g. /tmp) supplied via --socket.
    if !runtime_dir.exists() {
        std::fs::create_dir_all(&runtime_dir)?;
        std::fs::set_permissions(&runtime_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    // Remove a stale socket left by a crashed server.
    if config.socket_path.exists() {
        if UnixStream::connect(&config.socket_path).await.is_ok() {
            return Err(format!(
                "a server is already listening on {}",
                config.socket_path.display()
            )
            .into());
        }
        std::fs::remove_file(&config.socket_path)?;
    }

    let store: SharedStore = Arc::new(std::sync::RwLock::new(Store::new()));
    let (bcast, _) = broadcast::channel::<StoredEvent>(4096);
    let persist_pending = Arc::new(AtomicBool::new(false));
    let ptys = PtyManager::new(store.clone(), bcast.clone(), persist_pending.clone())
        .with_integration_home(config.integration_home.clone());

    // Restore previous snapshot (structure only — never processes).
    if let Some(loaded) = crate::persist::load(&config) {
        let n = loaded.snapshot.panes.len();
        crate::persist::apply(&store, &ptys.terms(), loaded);
        tracing::info!("restored {n} panes from snapshot (no processes restarted)");
    }

    let listener = UnixListener::bind(&config.socket_path)?;
    std::fs::set_permissions(&config.socket_path, std::fs::Permissions::from_mode(0o600))?;
    tracing::info!("listening on {}", config.socket_path.display());

    let shutdown = Arc::new(tokio::sync::Notify::new());
    let plugins = signaltty_plugin::PluginRegistry::load(config.plugin_dir.clone());
    let overlays = crate::router::load_overlays(&config.agents_dir);
    let audit = crate::audit::AuditLog::open(&config.state_dir)?;
    let sequence = crate::audit::EventSequence::open(&config.state_dir, audit.max_seq())?;
    store
        .write()
        .unwrap()
        .configure_events(audit, sequence, bcast.clone());
    crate::persist::recover_tasks(&store);
    let ctx = Arc::new(Ctx {
        store: store.clone(),
        approvals: crate::approvals::Approvals::default(),
        worktrees: crate::worktrees::Worktrees::default(),
        bcast,
        ptys: ptys.clone(),
        config: config.clone(),
        shutdown: shutdown.clone(),
        plugins,
        overlays,
    });

    // Plugin event hooks: every broadcast event (except high-volume
    // pty.data) is offered to matching hooks as JSON on stdin.
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut rx = ctx.bcast.subscribe();
            loop {
                match rx.recv().await {
                    Ok(ev) => {
                        if ev.name == signaltty_proto::event::PTY_DATA {
                            continue;
                        }
                        let envelope =
                            serde_json::to_value(EventMsg::new(&ev.name, ev.seq, ev.payload))
                                .unwrap_or(serde_json::Value::Null);
                        ctx.plugins
                            .dispatch(&ev.name, &envelope, &ctx.config.socket_path)
                            .await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }

    // Live /proc refresh (docs/07 layer 4): promote agent kinds and follow
    // cwds every 10s. Broadcasts + persists only when something changed.
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                let events =
                    crate::procscan::scan(&ctx.store, &ctx.ptys.child_pids(), &ctx.overlays);
                if !events.is_empty() {
                    ctx.mark_persist();
                }
            }
        });
    }

    // Silent-worker watchdog:
    // If a task stays `working` with no hook activity and no PTY output
    // for worker_silent_timeout_s, transition to `input_required` with
    // {reason: "worker_silent"}
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
            loop {
                interval.tick().await;
                let timeout_s = ctx.config.worker_silent_timeout_s;
                if timeout_s == 0 {
                    continue;
                }
                let now = chrono::Utc::now();
                let silent_tasks: Vec<(String, String)> = {
                    let s = ctx.store.read().unwrap();
                    s.tasks
                        .values()
                        .filter_map(|t| {
                            if t.state == signaltty_core::TaskState::Working {
                                if let Some(pid) = &t.pane_id {
                                    if let Some(p) = s.panes.get(pid) {
                                        let elapsed = now
                                            .signed_duration_since(p.last_activity_at)
                                            .num_seconds();
                                        if elapsed >= timeout_s as i64 {
                                            return Some((t.id.clone(), pid.clone()));
                                        }
                                    }
                                }
                            }
                            None
                        })
                        .collect()
                };

                if silent_tasks.is_empty() {
                    continue;
                }
                let mut s = ctx.store.write().unwrap();
                for (task_id, _pid) in silent_tasks {
                    if let Some(t) = s.tasks.get(&task_id) {
                        if t.state == signaltty_core::TaskState::Working {
                            let evidence = serde_json::json!({
                                "reason": "worker_silent",
                                "timeout_s": timeout_s,
                            });
                            s.task_input_required_on_turn_end(&task_id, Some(evidence));
                        }
                    }
                }
                drop(s);
                ctx.mark_persist();
            }
        });
    }

    // Debounced snapshot flusher.
    {
        let ctx = ctx.clone();
        let pending = persist_pending.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                if pending.swap(false, Ordering::Relaxed) {
                    if let Err(e) = crate::persist::save(&ctx.store, &ctx.ptys.terms(), &ctx.config)
                    {
                        tracing::warn!("snapshot failed: {e}");
                    }
                }
            }
        });
    }

    // OS signals → snapshot + shutdown.
    {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate());
            let ctrlc = tokio::signal::ctrl_c();
            tokio::pin!(ctrlc);
            match sigterm {
                Ok(ref mut term) => {
                    tokio::select! {
                        _ = term.recv() => {},
                        _ = &mut ctrlc => {},
                    }
                }
                Err(_) => {
                    ctrlc.await.ok();
                }
            }
            tracing::info!("shutting down; writing snapshot");
            if let Err(e) = crate::persist::save(&ctx.store, &ctx.ptys.terms(), &ctx.config) {
                tracing::warn!("shutdown snapshot failed: {e}");
            }
            ctx.shutdown.notify_waiters();
        });
    }

    let my_uid = nix::unistd::getuid().as_raw();
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                tracing::info!("shutdown requested");
                break;
            }
            conn = listener.accept() => {
                let (stream, _) = match conn {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("accept failed: {e}");
                        continue;
                    }
                };
                // SO_PEERCRED: only our own uid, even if perms were loosened.
                match stream.peer_cred() {
                    Ok(cred) if cred.uid() == my_uid => {}
                    Ok(cred) => {
                        tracing::warn!("refusing connection from uid {}", cred.uid());
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!("peer_cred failed: {e}");
                        continue;
                    }
                }
                let ctx = ctx.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(ctx, stream).await {
                        tracing::debug!("connection ended: {e}");
                    }
                });
            }
        }
    }
    ctx.ptys.cancel_inputs();
    Ok(())
}

struct ConnState {
    ptys: PtyManager,
    subs: Vec<String>,
    task_ids: Option<Vec<String>>,
    attached: HashSet<String>,
    fence: u64,
    last_received: u64,
}

impl Drop for ConnState {
    fn drop(&mut self) {
        for pane_id in &self.attached {
            self.ptys.remove_viewer(pane_id);
        }
    }
}

async fn handle_conn(ctx: Arc<Ctx>, stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    let (read_half, write_half) = stream.into_split();
    // `next_line` is cancel-safe; `read_line` would drop a partly received
    // request whenever an event wins the select below.
    let mut lines = BufReader::new(read_half).lines();
    let writer = Arc::new(Mutex::new(write_half));
    let mut rx = ctx.bcast.subscribe();
    let mut state = ConnState {
        ptys: ctx.ptys.clone(),
        subs: Vec::new(),
        task_ids: None,
        attached: HashSet::new(),
        fence: 0,
        last_received: 0,
    };

    let send_line = |writer: Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>, line: String| async move {
        let mut w = writer.lock().await;
        w.write_all(line.as_bytes()).await
    };

    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else {
                    break; // EOF = implicit detach.
                };
                if line.len() > MAX_LINE_BYTES {
                    let resp = Response::err("?", code::RATE_LIMITED, "line too large");
                    send_line(writer.clone(), resp.to_line()).await?;
                    continue;
                }
                let req: Request = match serde_json::from_str(line.trim()) {
                    Ok(r) => r,
                    Err(_) => {
                        let resp = Response::err("?", code::BAD_PARAMS, "invalid JSON");
                        send_line(writer.clone(), resp.to_line()).await?;
                        continue;
                    }
                };
                if req.protocol != signaltty_proto::PROTOCOL {
                    let resp = Response::err(&req.id, code::BAD_PROTOCOL,
                        format!("want {}, got {}", signaltty_proto::PROTOCOL, req.protocol));
                    send_line(writer.clone(), resp.to_line()).await?;
                    continue;
                }
                let native_wait = req.method == signaltty_proto::method::HOOK_EVENT
                    && req.params.get("wait_for_answer").and_then(serde_json::Value::as_bool) == Some(true);
                let is_wait_call = native_wait
                    || req.method == signaltty_proto::method::WAIT
                    || req.method == signaltty_proto::method::TASK_WAIT;
                let (mut resp, effect) = if is_wait_call {
                    tokio::select! {
                        result = dispatch(&ctx, &req) => result,
                        _ = lines.next_line() => break,
                        _ = ctx.shutdown.notified() => break,
                    }
                } else { dispatch(&ctx, &req).await };
                // Register streaming before the final snapshot. Output is either
                // covered by its offset or delivered after the response.
                for pane_id in effect.attach {
                    if state.attached.insert(pane_id.clone()) {
                        ctx.ptys.add_viewer(&pane_id);
                    }
                    let (snapshot, output_offset) = ctx.ptys.snapshot(&pane_id);
                    use base64::Engine;
                    resp.result["snapshot_b64"] = serde_json::json!(base64::engine::general_purpose::STANDARD.encode(snapshot));
                    resp.result["output_offset"] = serde_json::json!(output_offset);
                }
                send_line(writer.clone(), resp.to_line()).await?;
                // Apply the remaining connection effects.
                for pane_id in effect.detach {
                    if state.attached.remove(&pane_id) {
                        ctx.ptys.remove_viewer(&pane_id);
                    }
                }
                if let Some(subs) = effect.subscribe {
                    state.subs = subs;
                }
                if let Some(task_ids) = effect.task_ids {
                    state.task_ids = Some(task_ids);
                }
                if let Some(fence) = effect.fence { state.fence = fence; }
                if effect.close { break; }
                if let Some(backlog) = effect.replay {
                    for ev in backlog {
                        let msg = EventMsg::new(&ev.name, ev.seq, ev.payload);
                        send_line(writer.clone(), msg.to_line()).await?;
                    }
                }
            }
            msg = rx.recv() => {
                let ev = match msg {
                    Ok(ev) => ev,
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // A long `wait`/`hook-event` dispatch does not poll this
                        // receiver. When nothing was deliverable, lag is harmless:
                        // later subscriptions replay from their own fence.
                        if state.subs.is_empty() && state.attached.is_empty() {
                            rx = rx.resubscribe();
                            continue;
                        }
                        break;
                    }
                    Err(_) => break,
                };
                if ev.seq <= state.last_received { break; }
                state.last_received = ev.seq;
                if ev.name != signaltty_proto::event::PTY_DATA && ev.seq <= state.fence { continue; }
                let pane_id = ev.payload.get("pane_id").and_then(|v| v.as_str());
                let for_attached = ev.name == signaltty_proto::event::PTY_DATA
                    && pane_id.map(|p| state.attached.contains(p)).unwrap_or(false);
                let for_sub = state.subs.iter().any(|g| signaltty_proto::glob_matches(g, &ev.name));
                let for_task = if let Some(ref tids) = state.task_ids {
                    if ev.name.starts_with("task.") {
                        ev.payload
                            .get("task_id")
                            .and_then(|v| v.as_str())
                            .map(|tid| tids.iter().any(|t| t == tid))
                            .unwrap_or(false)
                    } else {
                        true
                    }
                } else {
                    true
                };
                if for_attached || (for_sub && for_task && ev.name != signaltty_proto::event::PTY_DATA) {
                    let msg = EventMsg::new(&ev.name, ev.seq, ev.payload);
                    if send_line(writer.clone(), msg.to_line()).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> (Arc<Ctx>, std::path::PathBuf, String) {
        let base = std::env::temp_dir().join(signaltty_core::ids::new_pane_id());
        let config = Config {
            socket_path: base.join("test.sock"),
            state_dir: base.join("state"),
            history_tail_bytes: 1024,
            plugin_dir: base.join("plugins"),
            agents_dir: base.join("agents"),
            integration_home: Some(base.join("home")),
            max_parallel_tasks: crate::config::DEFAULT_MAX_PARALLEL_TASKS,
            worker_silent_timeout_s: crate::config::DEFAULT_WORKER_SILENT_TIMEOUT_S,
            merge_timeout_ms: crate::config::DEFAULT_MERGE_TIMEOUT_MS,
            fetch_timeout_ms: crate::config::DEFAULT_FETCH_TIMEOUT_MS,
        };
        let mut store = Store::new();
        let pane = signaltty_core::model::Pane::new(
            "ws".into(),
            "tab".into(),
            "/tmp".into(),
            vec!["sleep".into()],
            signaltty_core::model::PtySize::default(),
            chrono::Utc::now(),
        );
        let id = pane.id.clone();
        store.panes.insert(id.clone(), pane);
        let (bcast, _) = broadcast::channel(8);
        let audit = crate::audit::AuditLog::open(&config.state_dir).unwrap();
        let sequence = crate::audit::EventSequence::open(&config.state_dir, 0).unwrap();
        store.configure_events(audit, sequence, bcast.clone());
        let store = Arc::new(std::sync::RwLock::new(store));
        let ptys = PtyManager::new(
            store.clone(),
            bcast.clone(),
            Arc::new(AtomicBool::new(false)),
        );
        let ctx = Arc::new(Ctx {
            store,
            bcast,
            ptys,
            config: config.clone(),
            shutdown: Arc::new(tokio::sync::Notify::new()),
            approvals: crate::approvals::Approvals::default(),
            worktrees: crate::worktrees::Worktrees::default(),
            plugins: signaltty_plugin::PluginRegistry::load(config.plugin_dir),
            overlays: Vec::new(),
        });
        (ctx, base, id)
    }

    #[tokio::test]
    async fn pending_wait_releases_receivers_on_eof_shutdown_and_another_request() {
        for cause in ["eof", "shutdown", "request"] {
            let (ctx, base, pane) = test_context();
            let (mut client, server) = UnixStream::pair().unwrap();
            let owned = ctx.clone();
            let task =
                tokio::spawn(
                    async move { handle_conn(owned, server).await.map_err(|e| e.to_string()) },
                );
            let request = serde_json::json!({"protocol":signaltty_proto::PROTOCOL,"id":"wait","method":"wait","params":{"pane_id":pane,"until":"done","timeout_s":3600}});
            client
                .write_all(format!("{request}\n").as_bytes())
                .await
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while ctx.bcast.receiver_count() != 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            match cause {
                "eof" => drop(client),
                "shutdown" => ctx.shutdown.notify_waiters(),
                _ => {
                    client.write_all(b"{}\n").await.unwrap();
                }
            }
            tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(ctx.bcast.receiver_count(), 0, "{cause}");
            std::fs::remove_dir_all(base).unwrap();
        }
    }

    #[tokio::test]
    async fn pending_task_wait_releases_receivers_on_eof() {
        let (ctx, base, _pane) = test_context();
        let task_id = {
            let mut store = ctx.store.write().unwrap();
            let now = chrono::Utc::now();
            let id = signaltty_core::ids::new_task_id();
            store.tasks.insert(
                id.clone(),
                signaltty_core::model::Task {
                    id: id.clone(),
                    context_id: signaltty_core::ids::new_context_id(),
                    parent_task_id: None,
                    pane_id: None,
                    parent_pane_id: None,
                    root_pane_id: None,
                    relationship: signaltty_core::model::Relationship::Subagent,
                    label: "wait-eof".into(),
                    contract: signaltty_core::model::Contract::new("wait").unwrap(),
                    agent: None,
                    source_repo: std::path::PathBuf::from("/tmp/repo"),
                    target_branch: Some("main".into()),
                    worktree_path: std::path::PathBuf::from("/tmp/wt"),
                    branch: "task-wait".into(),
                    preexisting_branch: false,
                    base_ref: "HEAD".into(),
                    base_sha: "abc".into(),
                    state: signaltty_core::state::TaskState::Pending,
                    result: None,
                    disposition: signaltty_core::model::Disposition::default(),
                    pr: None,
                    status_reason: None,
                    finish_error: None,
                    worker_pid: None,
                    worker_cmd: None,
                    client_request_id: None,
                    created_at: now,
                    updated_at: now,
                },
            );
            id
        };
        let (mut client, server) = UnixStream::pair().unwrap();
        let owned = ctx.clone();
        let task =
            tokio::spawn(
                async move { handle_conn(owned, server).await.map_err(|e| e.to_string()) },
            );
        let request = serde_json::json!({
            "protocol": signaltty_proto::PROTOCOL,
            "id": "wait",
            "method": "task.wait",
            "params": {"task_id": task_id, "until": "completed", "timeout_s": 3600}
        });
        client
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while ctx.bcast.receiver_count() != 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(client);
        tokio::time::timeout(std::time::Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(ctx.bcast.receiver_count(), 0);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn lagged_connection_closes_instead_of_skipping_events() {
        let (ctx, base, _) = test_context();
        let (mut client, server) = UnixStream::pair().unwrap();
        let owned = ctx.clone();
        let task =
            tokio::spawn(
                async move { handle_conn(owned, server).await.map_err(|e| e.to_string()) },
            );
        client.write_all(format!("{}\n",serde_json::json!({"protocol":signaltty_proto::PROTOCOL,"id":"s","method":"subscribe","params":{"events":["*"]}})).as_bytes()).await.unwrap();
        let mut reader = BufReader::new(client);
        let mut ack = String::new();
        reader.read_line(&mut ack).await.unwrap();
        // The current-thread runtime cannot drain the receiver during this burst.
        for _ in 0..32 {
            ctx.store
                .write()
                .unwrap()
                .emit("test.event", serde_json::Value::Null);
        }
        let mut eof = String::new();
        assert_eq!(
            tokio::time::timeout(
                std::time::Duration::from_secs(1),
                reader.read_line(&mut eof)
            )
            .await
            .unwrap()
            .unwrap(),
            0
        );
        task.await.unwrap().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn lagged_idle_connection_survives_and_serves_later_calls() {
        let (ctx, base, _) = test_context();
        let (mut client, server) = UnixStream::pair().unwrap();
        let owned = ctx.clone();
        let task =
            tokio::spawn(
                async move { handle_conn(owned, server).await.map_err(|e| e.to_string()) },
            );
        while ctx.bcast.receiver_count() != 1 {
            tokio::task::yield_now().await;
        }
        // No subscriptions or attachments: this burst overflows the receiver
        // without anything deliverable to this connection.
        for _ in 0..32 {
            ctx.store
                .write()
                .unwrap()
                .emit("test.event", serde_json::Value::Null);
        }
        client.write_all(format!("{}\n",serde_json::json!({"protocol":signaltty_proto::PROTOCOL,"id":"s","method":"server.status","params":{}})).as_bytes()).await.unwrap();
        let mut reader = BufReader::new(client);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let resp: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(resp["id"], "s");
        assert_eq!(resp["ok"], true);
        drop(reader);
        task.await.unwrap().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test]
    async fn replay_to_live_handoff_delivers_each_event_once() {
        let (ctx, base, _) = test_context();
        let (mut client, server) = UnixStream::pair().unwrap();
        let owned = ctx.clone();
        let task =
            tokio::spawn(
                async move { handle_conn(owned, server).await.map_err(|e| e.to_string()) },
            );
        while ctx.bcast.receiver_count() != 1 {
            tokio::task::yield_now().await;
        }
        for _ in 0..2 {
            ctx.store
                .write()
                .unwrap()
                .emit("test.event", serde_json::Value::Null);
        }
        client.write_all(format!("{}\n",serde_json::json!({"protocol":signaltty_proto::PROTOCOL,"id":"s","method":"subscribe","params":{"events":["test.*"],"from_seq":0}})).as_bytes()).await.unwrap();
        let mut reader = BufReader::new(client);
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        let ack: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(ack["result"]["replay"]["status"], "complete");
        assert_eq!(ack["result"]["seq"], 2);
        for seq in 1..=2 {
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let event: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(event["seq"], seq);
        }
        ctx.store
            .write()
            .unwrap()
            .emit("test.event", serde_json::Value::Null);
        line.clear();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap()["seq"],
            3
        );
        drop(reader);
        task.await.unwrap().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn dropping_a_connection_unregisters_all_viewers_even_on_error() {
        let store = Arc::new(std::sync::RwLock::new(crate::store::Store::new()));
        let (events, _) = tokio::sync::broadcast::channel(8);
        let ptys = crate::pty::PtyManager::new(store, events, Arc::new(AtomicBool::new(false)));
        ptys.add_viewer("pane");
        // Another connection remains attached to the same pane.
        ptys.add_viewer("pane");
        {
            let _connection = ConnState {
                ptys: ptys.clone(),
                subs: Vec::new(),
                task_ids: None,
                attached: HashSet::from(["pane".to_string()]),
                fence: 0,
                last_received: 0,
            };
        }
        assert_eq!(ptys.viewer_count("pane"), 1);
    }
}
