//! IPC actor: background thread owning the socket connections.
//!
//! Two connections: `control` for sequential request/response calls,
//! `sub` for the event subscription plus pane attach streaming.
//! The UI talks to the actor via mpsc; the actor pushes UI events
//! through a glib channel. Reconnects transparently and tells the UI
//! to refetch.
//!
//! The session loop waits on UI requests and the event stream at once,
//! so a call (a keystroke, a split) is served the moment it arrives —
//! the UI thread blocks on replies, so any polling here is felt as
//! input latency.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use signaltty_proto::{Request, Response};

/// Thread-safe UI event sender (glib 0.22 removed MainContext::channel).
pub type UiTx = tokio::sync::mpsc::UnboundedSender<UiEvent>;

pub struct SnapshotReply {
    pub snapshot: Vec<u8>,
}

pub enum ActorRequest {
    Call {
        method: String,
        params: Value,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Attach {
        pane_id: String,
        cols: u16,
        rows: u16,
        reply: oneshot::Sender<Result<SnapshotReply, String>>,
    },
    Detach {
        pane_id: String,
    },
}

#[derive(Debug)]
pub enum UiEvent {
    ServerEvent {
        name: String,
        payload: Value,
    },
    PtyData {
        pane_id: String,
        data: Vec<u8>,
    },
    /// Desktop-notification click (sent by notif.rs, not the actor).
    FocusPane(String),
    /// Desktop-notification "Mark read" (clears attention in place).
    MarkSeen(String),
    Reconnected,
    Disconnected,
}

#[derive(Clone)]
pub struct IpcHandle {
    tx: mpsc::UnboundedSender<ActorRequest>,
}

impl IpcHandle {
    #[cfg(test)]
    pub fn test_channel() -> (Self, mpsc::UnboundedReceiver<ActorRequest>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ActorRequest::Call {
                method: method.to_string(),
                params,
                reply: tx,
            })
            .map_err(|_| "ipc actor gone".to_string())?;
        rx.blocking_recv()
            .map_err(|_| "ipc reply lost".to_string())?
    }

    pub fn attach(&self, pane_id: &str, cols: u16, rows: u16) -> Result<SnapshotReply, String> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(ActorRequest::Attach {
                pane_id: pane_id.to_string(),
                cols,
                rows,
                reply: tx,
            })
            .map_err(|_| "ipc actor gone".to_string())?;
        rx.blocking_recv()
            .map_err(|_| "ipc reply lost".to_string())?
    }

    pub fn detach(&self, pane_id: &str) {
        let _ = self.tx.send(ActorRequest::Detach {
            pane_id: pane_id.to_string(),
        });
    }
}

pub fn spawn(socket: PathBuf, ui: UiTx) -> IpcHandle {
    let (tx, rx) = mpsc::unbounded_channel::<ActorRequest>();
    std::thread::Builder::new()
        .name("signaltty-ipc".to_string())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(actor_main(socket, rx, ui));
        })
        .expect("spawn ipc thread");
    IpcHandle { tx }
}

struct Conn {
    /// `next_line` is cancel-safe, so it can race UI requests.
    lines: Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl Conn {
    async fn connect(socket: &PathBuf) -> Result<Conn, String> {
        let stream = UnixStream::connect(socket)
            .await
            .map_err(|e| e.to_string())?;
        let (r, w) = stream.into_split();
        Ok(Conn {
            lines: BufReader::new(r).lines(),
            writer: w,
            next_id: 1,
        })
    }

    async fn send(&mut self, method: &str, params: Value) -> Result<String, String> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let req = Request {
            protocol: signaltty_proto::PROTOCOL.to_string(),
            id: id.clone(),
            method: method.to_string(),
            params,
        };
        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        crate::metrics::record("ipc", method);
        Ok(id)
    }

    async fn read_line(&mut self) -> Result<String, String> {
        match self.lines.next_line().await {
            Ok(Some(line)) => Ok(line),
            Ok(None) => Err("eof".to_string()),
            Err(e) => Err(e.to_string()),
        }
    }
}

enum Pending {
    Attach(oneshot::Sender<Result<SnapshotReply, String>>),
}

async fn actor_main(socket: PathBuf, mut rx: mpsc::UnboundedReceiver<ActorRequest>, ui: UiTx) {
    let mut attached: HashMap<String, (u16, u16)> = HashMap::new();
    loop {
        if run_session(&socket, &mut rx, &ui, &mut attached).await {
            break; // actor handle dropped: shut down
        }
        let _ = ui.send(UiEvent::Disconnected);
        // Reconnect loop (unless handle dropped).
        loop {
            // Drain queued requests with a controlled error.
            match rx.try_recv() {
                Err(mpsc::error::TryRecvError::Disconnected) => return,
                Err(mpsc::error::TryRecvError::Empty) => {}
                Ok(req) => {
                    reply_gone(req);
                    continue;
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            if Conn::connect(&socket).await.is_ok() {
                break;
            }
        }
        let _ = ui.send(UiEvent::Reconnected);
    }
}

fn reply_gone(req: ActorRequest) {
    match req {
        ActorRequest::Call { reply, .. } => {
            let _ = reply.send(Err("reconnecting".to_string()));
        }
        ActorRequest::Attach { reply, .. } => {
            let _ = reply.send(Err("reconnecting".to_string()));
        }
        ActorRequest::Detach { .. } => {}
    }
}

/// Run one connected session. Returns true to shut the actor down.
async fn run_session(
    socket: &PathBuf,
    rx: &mut mpsc::UnboundedReceiver<ActorRequest>,
    ui: &UiTx,
    attached: &mut HashMap<String, (u16, u16)>,
) -> bool {
    let mut control = match Conn::connect(socket).await {
        Ok(c) => c,
        Err(_) => return false,
    };
    let mut sub = match Conn::connect(socket).await {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Subscribe to everything (pty.data filtered server-side by attach).
    if sub
        .send("subscribe", json!({"events": ["*"]}))
        .await
        .is_err()
    {
        return false;
    }
    // Re-attach surviving panes (snapshotless; UI refetches on Reconnected).
    // mark_seen=false: visibility alone must not clear attention.
    for (pane_id, (cols, rows)) in attached.iter() {
        let _ = sub
            .send(
                "pane.attach",
                json!({"pane_id": pane_id, "cols": cols, "rows": rows, "mark_seen": false}),
            )
            .await;
    }
    let mut pending: HashMap<String, Pending> = HashMap::new();

    enum Next {
        Request(Option<ActorRequest>),
        Line(Result<String, String>),
    }
    loop {
        let next = tokio::select! {
            req = rx.recv() => Next::Request(req),
            line = sub.read_line() => Next::Line(line),
        };
        let line = match next {
            Next::Request(None) => return true, // handle dropped
            Next::Request(Some(req)) => {
                if handle_request(req, &mut control, &mut sub, attached, &mut pending).await {
                    return false; // eof
                }
                continue;
            }
            Next::Line(Ok(line)) => line,
            Next::Line(Err(_)) => return false, // eof
        };
        let v: Value = match serde_json::from_str(line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if v.get("type").and_then(|t| t.as_str()) == Some("event") {
            let name = v
                .get("event")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();
            let payload = v.get("payload").cloned().unwrap_or(Value::Null);
            if name == signaltty_proto::event::PTY_DATA {
                if let (Some(pid), Some(b64)) = (
                    payload.get("pane_id").and_then(|p| p.as_str()),
                    payload.get("data_b64").and_then(|d| d.as_str()),
                ) {
                    if let Ok(data) = base64_decode(b64) {
                        let _ = ui.send(UiEvent::PtyData {
                            pane_id: pid.to_string(),
                            data,
                        });
                    }
                }
            } else {
                let _ = ui.send(UiEvent::ServerEvent { name, payload });
            }
            continue;
        }
        // Response on sub connection: match pending attach.
        if let Ok(resp) = serde_json::from_value::<Response>(v) {
            if let Some(Pending::Attach(reply)) = pending.remove(&resp.id) {
                let out = if resp.ok {
                    let snap = resp
                        .result
                        .get("snapshot_b64")
                        .and_then(|s| s.as_str())
                        .and_then(|s| base64_decode(s).ok())
                        .unwrap_or_default();
                    Ok(SnapshotReply { snapshot: snap })
                } else {
                    Err(resp
                        .error
                        .map(|e| format!("{}: {}", e.code, e.message))
                        .unwrap_or_else(|| "error".to_string()))
                };
                let _ = reply.send(out);
            }
        }
    }
}

/// Handle one UI request. Returns true on connection EOF.
async fn handle_request(
    req: ActorRequest,
    control: &mut Conn,
    sub: &mut Conn,
    attached: &mut HashMap<String, (u16, u16)>,
    pending: &mut HashMap<String, Pending>,
) -> bool {
    match req {
        ActorRequest::Call {
            method,
            params,
            reply,
        } => {
            let out = match control.send(&method, params).await {
                Ok(_) => read_response(control).await,
                Err(e) => Err(e),
            };
            let eof = out_is_eof(&out);
            let _ = reply.send(out);
            eof
        }
        ActorRequest::Attach {
            pane_id,
            cols,
            rows,
            reply,
        } => {
            match sub
                .send(
                    "pane.attach",
                    json!({"pane_id": pane_id, "cols": cols, "rows": rows, "mark_seen": false}),
                )
                .await
            {
                Ok(id) => {
                    attached.insert(pane_id, (cols, rows));
                    pending.insert(id, Pending::Attach(reply));
                    false
                }
                Err(_) => {
                    let _ = reply.send(Err("eof".to_string()));
                    true
                }
            }
        }
        ActorRequest::Detach { pane_id } => {
            attached.remove(&pane_id);
            sub.send("pane.detach", json!({"pane_id": pane_id}))
                .await
                .is_err()
        }
    }
}

async fn read_response(control: &mut Conn) -> Result<Value, String> {
    loop {
        let line = control.read_line().await?;
        let v: Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
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

fn out_is_eof(out: &Result<Value, String>) -> bool {
    matches!(out, Err(e) if e == "eof")
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}
