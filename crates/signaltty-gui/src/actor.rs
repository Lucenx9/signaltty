//! IPC actor owns control and subscription sockets on a background runtime.
//! GTK awaits replies without blocking; stream delivery continues during calls.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{timeout, Instant};

use signaltty_proto::{Request, Response};

const IPC_TIMEOUT: Duration = Duration::from_secs(3);
const WORKTREE_TIMEOUT: Duration = Duration::from_secs(90);

fn worktree_call(method: &str) -> bool {
    matches!(
        method,
        "worktree.list" | "worktree.create" | "worktree.open" | "worktree.remove"
    )
}
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

pub type UiTx = mpsc::UnboundedSender<UiEvent>;

type Reply = oneshot::Sender<Result<Value, String>>;

pub enum ActorRequest {
    Call {
        method: String,
        params: Value,
        reply: Reply,
        deadline: Instant,
    },
    Attach {
        pane_id: String,
        cols: u16,
        rows: u16,
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
    /// Canonical VT state replaces the terminal contents before following data.
    PtySnapshot {
        pane_id: String,
        data: Vec<u8>,
    },
    PtyData {
        pane_id: String,
        data: Vec<u8>,
    },
    FocusPane(String),
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

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(ActorRequest::Call {
                method: method.into(),
                params,
                reply,
                deadline: Instant::now()
                    + if worktree_call(method) {
                        WORKTREE_TIMEOUT
                    } else {
                        IPC_TIMEOUT
                    },
            })
            .map_err(|_| "ipc actor gone".to_string())?;
        rx.await.map_err(|_| "ipc reply lost".to_string())?
    }

    pub fn attach(&self, pane_id: &str, cols: u16, rows: u16) {
        let _ = self.tx.send(ActorRequest::Attach {
            pane_id: pane_id.into(),
            cols,
            rows,
        });
    }

    pub fn detach(&self, pane_id: &str) {
        let _ = self.tx.send(ActorRequest::Detach {
            pane_id: pane_id.into(),
        });
    }
}

pub fn spawn(socket: PathBuf, ui: UiTx) -> IpcHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("signaltty-ipc".into())
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
    lines: Lines<BufReader<tokio::net::unix::OwnedReadHalf>>,
    writer: tokio::net::unix::OwnedWriteHalf,
    next_id: u64,
}

impl Conn {
    async fn connect(socket: &PathBuf) -> Result<Self, String> {
        let stream = timeout(IPC_TIMEOUT, UnixStream::connect(socket))
            .await
            .map_err(|_| "ipc connect timed out".to_string())?
            .map_err(|e| e.to_string())?;
        let (r, writer) = stream.into_split();
        Ok(Self {
            lines: BufReader::new(r).lines(),
            writer,
            next_id: 1,
        })
    }

    async fn send(&mut self, method: &str, params: Value) -> Result<String, String> {
        let id = self.next_id.to_string();
        self.next_id += 1;
        let req = Request {
            protocol: signaltty_proto::PROTOCOL.into(),
            id: id.clone(),
            method: method.into(),
            params,
        };
        let mut line = serde_json::to_string(&req).unwrap();
        line.push('\n');
        timeout(IPC_TIMEOUT, self.writer.write_all(line.as_bytes()))
            .await
            .map_err(|_| "ipc write timed out".to_string())?
            .map_err(|e| e.to_string())?;
        crate::metrics::record("ipc", method);
        Ok(id)
    }

    async fn read_line(&mut self) -> Result<String, String> {
        self.lines
            .next_line()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "eof".into())
    }
}

struct QueuedCall {
    method: String,
    params: Value,
    reply: Reply,
    deadline: Instant,
}
struct PendingCall {
    id: String,
    reply: Reply,
    deadline: Instant,
}
enum PendingKind {
    Attach(String),
    Detach,
}
struct Pending {
    kind: PendingKind,
    deadline: Instant,
}

async fn actor_main(socket: PathBuf, mut rx: mpsc::UnboundedReceiver<ActorRequest>, ui: UiTx) {
    let mut attached = HashMap::new();
    let mut reconnecting = false;
    loop {
        if run_session(&socket, &mut rx, &ui, &mut attached, reconnecting).await {
            return;
        }
        reconnecting = true;
        let _ = ui.send(UiEvent::Disconnected);
        let retry_at = Instant::now() + RECONNECT_DELAY;
        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(retry_at) => break,
                req = rx.recv() => match req {
                    None => return,
                    Some(ActorRequest::Call { reply, .. }) => { let _ = reply.send(Err("reconnecting".into())); }
                    Some(ActorRequest::Attach { pane_id, cols, rows }) => { attached.insert(pane_id, (cols, rows)); }
                    Some(ActorRequest::Detach { pane_id }) => { attached.remove(&pane_id); }
                }
            }
        }
    }
}

async fn run_session(
    socket: &PathBuf,
    rx: &mut mpsc::UnboundedReceiver<ActorRequest>,
    ui: &UiTx,
    attached: &mut HashMap<String, (u16, u16)>,
    reconnecting: bool,
) -> bool {
    // Bound the entire connect/subscribe phase, including queue wait for callers.
    let handshake = async {
        let control = Conn::connect(socket).await?;
        let mut sub = Conn::connect(socket).await?;
        let id = sub.send("subscribe", json!({"events":["*"]})).await?;
        let response = read_response(&mut sub, &id).await?;
        response_result(response)?;
        Ok::<_, String>((control, sub))
    };
    let (mut control, mut sub) = match timeout(IPC_TIMEOUT, handshake).await {
        Ok(Ok(connections)) => connections,
        _ => return false,
    };
    if reconnecting {
        let _ = ui.send(UiEvent::Reconnected);
    }
    let mut pending = HashMap::new();
    for (pane_id, &(cols, rows)) in attached.iter() {
        if send_attach(&mut sub, &mut pending, pane_id.clone(), cols, rows)
            .await
            .is_err()
        {
            return false;
        }
    }
    let mut offsets = HashMap::<String, u64>::new();
    let mut queued = VecDeque::<QueuedCall>::new();
    let mut current: Option<PendingCall> = None;
    loop {
        if current.is_none() {
            while let Some(call) = queued.pop_front() {
                if call.deadline <= Instant::now() {
                    let _ = call
                        .reply
                        .send(Err("ipc request timed out before sending".into()));
                    continue;
                }
                if call.reply.is_closed() {
                    continue;
                }
                match send_control(&mut control, &call).await {
                    Ok(id) => {
                        current = Some(PendingCall {
                            id,
                            reply: call.reply,
                            deadline: call.deadline,
                        });
                        break;
                    }
                    Err(e) => {
                        let _ = call.reply.send(Err(e));
                        return false;
                    }
                }
            }
        }
        let deadline = pending
            .values()
            .map(|p: &Pending| p.deadline)
            .chain(current.iter().map(|c| c.deadline))
            .chain(queued.iter().map(|c| c.deadline))
            .min()
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            req = rx.recv() => match req {
                None => return true,
                Some(ActorRequest::Call { method, params, reply, deadline }) => {
                    if worktree_call(&method) {
                        let socket = socket.clone();
                        tokio::spawn(async move {
                            let result = tokio::time::timeout_at(deadline, async {
                                let mut connection = Conn::connect(&socket).await?;
                                let id = connection.send(&method, params).await?;
                                let response = read_response(&mut connection, &id).await.map_err(|e| format!("ipc worktree response lost; outcome unknown: {e}"))?;
                                response_result(response)
                            }).await.unwrap_or_else(|_| Err("ipc worktree request timed out; outcome unknown".into()));
                            let _ = reply.send(result);
                        });
                    } else {
                        queued.push_back(QueuedCall { method, params, reply, deadline });
                    }
                }
                Some(ActorRequest::Attach { pane_id, cols, rows }) => {
                    attached.insert(pane_id.clone(), (cols, rows));
                    if send_attach(&mut sub, &mut pending, pane_id, cols, rows).await.is_err() { return false; }
                }
                Some(ActorRequest::Detach { pane_id }) => {
                    attached.remove(&pane_id);
                    offsets.remove(&pane_id);
                    match sub.send("pane.detach", json!({"pane_id":pane_id})).await {
                        Ok(id) => { pending.insert(id, Pending { kind:PendingKind::Detach, deadline:Instant::now() + IPC_TIMEOUT }); }
                        Err(_) => return false,
                    }
                }
            },
            line = control.read_line(), if current.is_some() => {
                let call = current.take().unwrap();
                let response = line.and_then(|line| parse_response(&line, &call.id));
                match response {
                    Ok(response) => { let _ = call.reply.send(response_result(response)); }
                    Err(e) => { let _ = call.reply.send(Err(e)); return false; }
                }
            },
            line = sub.read_line() => {
                let value: Value = match line.and_then(|line| serde_json::from_str(&line).map_err(|e| e.to_string())) { Ok(v) => v, Err(_) => return false };
                if value.get("type").and_then(Value::as_str) == Some("event") {
                    deliver_event(value, attached, &mut offsets, ui);
                    continue;
                }
                let response: Response = match serde_json::from_value(value) { Ok(r) => r, Err(_) => return false };
                if response.protocol != signaltty_proto::PROTOCOL { return false; }
                let Some(request) = pending.remove(&response.id) else { return false };
                if let PendingKind::Attach(pane_id) = request.kind {
                    if !attached.contains_key(&pane_id) { continue; }
                    if !response.ok {
                        if response.error.as_ref().map(|e| e.code.as_str()) == Some(signaltty_proto::code::NO_SUCH_PANE) {
                            attached.remove(&pane_id);
                            offsets.remove(&pane_id);
                        }
                        continue;
                    }
                    let Some(data) = response.result.get("snapshot_b64").and_then(Value::as_str).and_then(|s| base64_decode(s).ok()) else { return false };
                    if let Some(offset) = response.result.get("output_offset").and_then(Value::as_u64) { offsets.insert(pane_id.clone(), offset); }
                    let _ = ui.send(UiEvent::PtySnapshot { pane_id, data });
                }
            },
            _ = tokio::time::sleep_until(deadline) => {
                let now = Instant::now();
                let mut waiting = VecDeque::new();
                for call in queued.drain(..) {
                    if call.deadline <= now {
                        let _ = call.reply.send(Err("ipc request timed out before sending".into()));
                    } else if !call.reply.is_closed() {
                        waiting.push_back(call);
                    }
                }
                queued = waiting;
                if current.as_ref().is_some_and(|call| call.deadline <= now) {
                    let _ = current.take().unwrap().reply.send(Err("ipc request timed out; outcome unknown".into()));
                    return false; // never associate a delayed reply with a later mutation
                }
                if pending.values().any(|p| p.deadline <= now) { return false; }
            }
        }
    }
}

async fn send_control(control: &mut Conn, call: &QueuedCall) -> Result<String, String> {
    timeout(
        call.deadline.saturating_duration_since(Instant::now()),
        control.send(&call.method, call.params.clone()),
    )
    .await
    .map_err(|_| "ipc request timed out; outcome unknown".to_string())?
}

async fn send_attach(
    sub: &mut Conn,
    pending: &mut HashMap<String, Pending>,
    pane_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let id = sub
        .send(
            "pane.attach",
            json!({"pane_id":pane_id,"cols":cols,"rows":rows,"mark_seen":false}),
        )
        .await?;
    pending.insert(
        id,
        Pending {
            kind: PendingKind::Attach(pane_id),
            deadline: Instant::now() + IPC_TIMEOUT,
        },
    );
    Ok(())
}

fn deliver_event(
    value: Value,
    attached: &HashMap<String, (u16, u16)>,
    offsets: &mut HashMap<String, u64>,
    ui: &UiTx,
) {
    let name = value
        .get("event")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let payload = value.get("payload").cloned().unwrap_or(Value::Null);
    if name != signaltty_proto::event::PTY_DATA {
        let _ = ui.send(UiEvent::ServerEvent { name, payload });
        return;
    }
    let Some(pane_id) = payload.get("pane_id").and_then(Value::as_str) else {
        return;
    };
    if !attached.contains_key(pane_id) {
        return;
    }
    let Some(mut data) = payload
        .get("data_b64")
        .and_then(Value::as_str)
        .and_then(|s| base64_decode(s).ok())
    else {
        return;
    };
    if let Some(end) = payload.get("output_offset").and_then(Value::as_u64) {
        let seen = offsets.entry(pane_id.to_string()).or_default();
        if end <= *seen {
            return;
        }
        let start = end.saturating_sub(data.len() as u64);
        let covered = seen.saturating_sub(start).min(data.len() as u64) as usize;
        data.drain(..covered);
        *seen = end;
    }
    if !data.is_empty() {
        let _ = ui.send(UiEvent::PtyData {
            pane_id: pane_id.into(),
            data,
        });
    }
}

async fn read_response(conn: &mut Conn, expected_id: &str) -> Result<Response, String> {
    parse_response(&conn.read_line().await?, expected_id)
}

fn parse_response(line: &str, expected_id: &str) -> Result<Response, String> {
    let response: Response = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if response.protocol != signaltty_proto::PROTOCOL {
        return Err("ipc response protocol mismatch".into());
    }
    if response.id != expected_id {
        return Err("ipc response id mismatch".into());
    }
    Ok(response)
}

fn response_result(response: Response) -> Result<Value, String> {
    if response.ok {
        return Ok(response.result);
    }
    Err(response
        .error
        .map(|e| format!("{}: {}", e.code, e.message))
        .unwrap_or_else(|| "invalid ipc error response".into()))
}

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s.as_bytes())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::net::UnixListener;

    static NEXT_SOCKET: AtomicU64 = AtomicU64::new(0);
    struct TestSocket(PathBuf);
    impl TestSocket {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "signaltty-actor-{}-{}.sock",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for TestSocket {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    async fn server_conn(listener: &UnixListener) -> Conn {
        let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let (r, writer) = stream.into_split();
        Conn {
            lines: BufReader::new(r).lines(),
            writer,
            next_id: 1,
        }
    }
    async fn request(conn: &mut Conn) -> Request {
        let line = tokio::time::timeout(Duration::from_secs(5), conn.read_line())
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&line).unwrap()
    }
    async fn respond(conn: &mut Conn, req: &Request, result: Value) {
        conn.writer
            .write_all(Response::ok(&req.id, result).to_line().as_bytes())
            .await
            .unwrap();
    }
    async fn connected(listener: &UnixListener) -> (Conn, Conn) {
        let control = server_conn(listener).await;
        let mut sub = server_conn(listener).await;
        let subscribe = request(&mut sub).await;
        assert_eq!(subscribe.method, "subscribe");
        respond(&mut sub, &subscribe, json!({"subscribed": true})).await;
        (control, sub)
    }
    async fn next_event(events: &mut mpsc::UnboundedReceiver<UiEvent>) -> UiEvent {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap()
    }
    async fn data(sub: &mut Conn, text: &str, offset: u64) {
        use base64::Engine;
        let value = json!({"type":"event", "event":signaltty_proto::event::PTY_DATA,
            "payload":{"pane_id":"pane-test", "data_b64":base64::engine::general_purpose::STANDARD.encode(text), "output_offset":offset}});
        sub.writer
            .write_all(format!("{value}\n").as_bytes())
            .await
            .unwrap();
    }
    async fn snapshot(sub: &mut Conn, text: &str, offset: u64) {
        use base64::Engine;
        let attach = request(sub).await;
        assert_eq!(attach.method, "pane.attach");
        assert_eq!(attach.params["mark_seen"], false);
        respond(sub, &attach, json!({"snapshot_b64":base64::engine::general_purpose::STANDARD.encode(text), "output_offset":offset})).await;
    }

    #[tokio::test]
    async fn stream_keeps_delivering_while_control_reply_is_stalled() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, mut events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (mut control, mut sub) = connected(&listener).await;
        let caller = actor.clone();
        let call = tokio::spawn(async move { caller.call("workspace.list", json!({})).await });
        let _ = request(&mut control).await;
        sub.writer
            .write_all(b"{\"type\":\"event\",\"event\":\"notification.created\",\"payload\":{}}\n")
            .await
            .unwrap();
        let event = tokio::time::timeout(Duration::from_millis(300), events.recv()).await;
        assert!(
            matches!(event, Ok(Some(UiEvent::ServerEvent { name, .. })) if name == "notification.created")
        );
        call.abort();
    }

    #[tokio::test]
    async fn reconnect_replays_snapshot_before_live_data_without_duplicate_bytes() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, mut events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (control, mut sub) = connected(&listener).await;
        actor.attach("pane-test", 80, 24);
        snapshot(&mut sub, "one", 3).await;
        assert!(
            matches!(next_event(&mut events).await, UiEvent::PtySnapshot { data, .. } if data == b"one")
        );
        data(&mut sub, "onetwo", 6).await;
        assert!(
            matches!(next_event(&mut events).await, UiEvent::PtyData { data, .. } if data == b"two")
        );
        data(&mut sub, "two", 6).await;
        sub.writer
            .write_all(b"{\"type\":\"event\",\"event\":\"notification.created\",\"payload\":{}}\n")
            .await
            .unwrap();
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::ServerEvent { .. }
        ));
        drop(control);
        drop(sub);
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Disconnected
        ));
        let (_control, mut sub) = connected(&listener).await;
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Reconnected
        ));
        snapshot(&mut sub, "one GAP two", 11).await;
        data(&mut sub, " GAP twoLIVE", 15).await;
        assert!(
            matches!(next_event(&mut events).await, UiEvent::PtySnapshot { data, .. } if data == b"one GAP two")
        );
        assert!(
            matches!(next_event(&mut events).await, UiEvent::PtyData { data, .. } if data == b"LIVE")
        );
    }

    #[tokio::test]
    async fn slow_worktree_uses_an_independent_connection_and_keeps_control_available() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, _events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (mut control, _sub) = connected(&listener).await;
        let caller = actor.clone();
        let worktree = tokio::spawn(async move {
            caller
                .call(
                    "worktree.create",
                    json!({"workspace_id": "a", "branch": "feature", "path": "/tmp/worktree"}),
                )
                .await
        });
        let mut dedicated = server_conn(&listener).await;
        let creation = request(&mut dedicated).await;
        assert_eq!(creation.method, "worktree.create");
        let caller = actor.clone();
        let fast = tokio::spawn(async move { caller.call("server.status", json!({})).await });
        let status = request(&mut control).await;
        assert_eq!(status.method, "server.status");
        respond(&mut control, &status, json!({"ready": true})).await;
        assert_eq!(fast.await.unwrap().unwrap()["ready"], true);
        tokio::time::sleep(IPC_TIMEOUT + Duration::from_millis(100)).await;
        respond(
            &mut dedicated,
            &creation,
            json!({"workspace": {"id": "new"}}),
        )
        .await;
        assert_eq!(worktree.await.unwrap().unwrap()["workspace"]["id"], "new");
    }

    #[tokio::test]
    async fn stalled_mutation_times_out_and_is_never_retried() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, mut events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (mut control, _sub) = connected(&listener).await;
        let caller = actor.clone();
        let call = tokio::spawn(async move { caller.call("pane.spawn", json!({})).await });
        assert_eq!(request(&mut control).await.method, "pane.spawn");
        let error = tokio::time::timeout(IPC_TIMEOUT + Duration::from_secs(1), call)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.contains("outcome unknown"), "{error}");
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Disconnected
        ));
        let (mut control, _sub) = connected(&listener).await;
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Reconnected
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), control.read_line())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn missing_panes_are_not_reattached_again() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, mut events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (control, mut sub) = connected(&listener).await;
        actor.attach("pane-gone", 80, 24);
        let attach = request(&mut sub).await;
        sub.writer
            .write_all(
                Response::err(&attach.id, signaltty_proto::code::NO_SUCH_PANE, "missing")
                    .to_line()
                    .as_bytes(),
            )
            .await
            .unwrap();
        sub.writer
            .write_all(b"{\"type\":\"event\",\"event\":\"notification.created\",\"payload\":{}}\n")
            .await
            .unwrap();
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::ServerEvent { .. }
        ));
        drop(control);
        drop(sub);
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Disconnected
        ));
        let (_control, mut sub) = connected(&listener).await;
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Reconnected
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), sub.read_line())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn mismatched_response_discards_connection_before_the_next_call() {
        let socket = TestSocket::new();
        let listener = UnixListener::bind(&socket.0).unwrap();
        let (ui, mut events) = mpsc::unbounded_channel();
        let actor = spawn(socket.0.clone(), ui);
        let (mut control, _sub) = connected(&listener).await;
        let caller = actor.clone();
        let call = tokio::spawn(async move { caller.call("pane.spawn", json!({})).await });
        let _ = request(&mut control).await;
        control
            .writer
            .write_all(
                Response::ok("wrong-id", json!({"pane":"wrong"}))
                    .to_line()
                    .as_bytes(),
            )
            .await
            .unwrap();
        assert!(call.await.unwrap().unwrap_err().contains("id mismatch"));
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Disconnected
        ));
        let (mut control, _sub) = connected(&listener).await;
        assert!(matches!(
            next_event(&mut events).await,
            UiEvent::Reconnected
        ));
        let caller = actor.clone();
        let call = tokio::spawn(async move { caller.call("pane.get", json!({})).await });
        let req = request(&mut control).await;
        assert_eq!(req.method, "pane.get");
        respond(&mut control, &req, json!({"pane":"fresh"})).await;
        assert_eq!(call.await.unwrap().unwrap(), json!({"pane":"fresh"}));
    }
}
