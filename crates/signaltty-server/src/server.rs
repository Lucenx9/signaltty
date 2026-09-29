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
    let ptys = PtyManager::new(store.clone(), bcast.clone(), persist_pending.clone());

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
    // The audit lives in the store so every emit is logged at one seam.
    store
        .write()
        .unwrap()
        .set_audit(crate::audit::AuditLog::open_or_disabled(&config.state_dir));
    let ctx = Arc::new(Ctx {
        store: store.clone(),
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
                    for ev in events {
                        let _ = ctx.bcast.send(ev);
                    }
                    ctx.mark_persist();
                }
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
    Ok(())
}

struct ConnState {
    subs: Vec<String>,
    attached: HashSet<String>,
}

async fn handle_conn(ctx: Arc<Ctx>, stream: UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    let (read_half, write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let writer = Arc::new(Mutex::new(write_half));
    let mut rx = ctx.bcast.subscribe();
    let mut state = ConnState {
        subs: Vec::new(),
        attached: HashSet::new(),
    };

    let send_line = |writer: Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>, line: String| async move {
        let mut w = writer.lock().await;
        w.write_all(line.as_bytes()).await
    };

    loop {
        let mut line = String::new();
        tokio::select! {
            n = reader.read_line(&mut line) => {
                let n = n?;
                if n == 0 {
                    break; // EOF = implicit detach.
                }
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
                let (resp, effect) = dispatch(&ctx, &req).await;
                send_line(writer.clone(), resp.to_line()).await?;
                // Apply connection effects.
                for pane_id in effect.attach {
                    ctx.ptys.add_viewer(&pane_id);
                    state.attached.insert(pane_id);
                }
                for pane_id in effect.detach {
                    ctx.ptys.remove_viewer(&pane_id);
                    state.attached.remove(&pane_id);
                }
                if let Some(subs) = effect.subscribe {
                    state.subs = subs;
                }
                if let Some(from) = effect.replay_from {
                    // Audit backfill (survives rotation + restart) merged
                    // with the ring, deduped by seq (see audit.rs).
                    let backlog = {
                        let s = ctx.store.read().unwrap();
                        let backfill = s.audit_since(from, crate::audit::REPLAY_CAP);
                        let ring = s.events_since(from);
                        crate::audit::merge_replay(backfill, ring)
                    };
                    for ev in backlog {
                        if state.subs.iter().any(|g| signaltty_proto::glob_matches(g, &ev.name)) {
                            let msg = EventMsg::new(&ev.name, ev.seq, ev.payload);
                            send_line(writer.clone(), msg.to_line()).await?;
                        }
                    }
                }
            }
            msg = rx.recv() => {
                let ev = match msg {
                    Ok(ev) => ev,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                let pane_id = ev.payload.get("pane_id").and_then(|v| v.as_str());
                let for_attached = ev.name == signaltty_proto::event::PTY_DATA
                    && pane_id.map(|p| state.attached.contains(p)).unwrap_or(false);
                let for_sub = state.subs.iter().any(|g| signaltty_proto::glob_matches(g, &ev.name));
                if for_attached || (for_sub && ev.name != signaltty_proto::event::PTY_DATA) {
                    let msg = EventMsg::new(&ev.name, ev.seq, ev.payload);
                    if send_line(writer.clone(), msg.to_line()).await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    // Implicit detach: drop viewer registrations.
    for pane_id in state.attached {
        ctx.ptys.remove_viewer(&pane_id);
    }
    Ok(())
}
