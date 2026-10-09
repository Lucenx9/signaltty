//! Real server + private D-Bus: never contacts the host's logind.
use std::io::{BufRead, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use signaltty_testkit::TestServer;

const DEADLINE: Duration = Duration::from_secs(8);

struct PrivateBus {
    child: Child,
    address: String,
}

impl PrivateBus {
    fn new() -> Self {
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon (installed by setup-dev)");
        let mut address = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self { child, address }
    }

    async fn manager(&self, preparing: bool) -> (zbus::Connection, Arc<Mutex<Option<UnixStream>>>) {
        self.manager_denying(preparing, 0).await
    }

    async fn manager_denying(
        &self,
        preparing: bool,
        deny_remaining: u32,
    ) -> (zbus::Connection, Arc<Mutex<Option<UnixStream>>>) {
        let peer = Arc::new(Mutex::new(None));
        let service = zbus::connection::Builder::address(self.address.trim())
            .unwrap()
            .name("org.freedesktop.login1")
            .unwrap()
            .serve_at(
                "/org/freedesktop/login1",
                LoginManager {
                    preparing,
                    peer: peer.clone(),
                    deny_remaining: AtomicU32::new(deny_remaining),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        (service, peer)
    }

    async fn server(&self) -> TestServer {
        TestServer::start_with_env(&[
            ("DBUS_SYSTEM_BUS_ADDRESS", self.address.trim()),
            ("SIGNALTTY_LOGIND", "1"),
        ])
        .await
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct LoginManager {
    preparing: bool,
    peer: Arc<Mutex<Option<UnixStream>>>,
    deny_remaining: AtomicU32,
}

#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn inhibit(
        &self,
        what: &str,
        who: &str,
        _why: &str,
        mode: &str,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
        assert_eq!((what, who, mode), ("shutdown", "signaltty", "delay"));
        if self
            .deny_remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(zbus::fdo::Error::AccessDenied("inhibit denied".into()));
        }
        let (lock, peer) = UnixStream::pair().unwrap();
        peer.set_nonblocking(true).unwrap();
        *self.peer.lock().unwrap() = Some(peer);
        Ok(std::os::fd::OwnedFd::from(lock).into())
    }

    #[zbus(property)]
    fn preparing_for_shutdown(&self) -> bool {
        self.preparing
    }
}

async fn inhibitor(peer: &Mutex<Option<UnixStream>>) -> UnixStream {
    tokio::time::timeout(DEADLINE, async {
        loop {
            if let Some(peer) = peer.lock().unwrap().take() {
                return peer;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("server acquired inhibitor")
}

async fn emit(service: &zbus::Connection, preparing: bool) {
    service
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "PrepareForShutdown",
            &preparing,
        )
        .await
        .unwrap();
}

async fn released(peer: &mut UnixStream) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            match peer.read(&mut [0]) {
                Ok(0) => return,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                other => panic!("unexpected inhibitor read: {other:?}"),
            }
        }
    })
    .await
    .expect("inhibitor released");
}

#[tokio::test]
async fn host_shutdown_saves_live_resume_metadata_before_releasing_inhibitor() {
    let bus = PrivateBus::new();
    let (service, lock) = bus.manager(false).await;
    let mut server = bus.server().await;
    let mut peer = inhibitor(&lock).await;
    let mut client = server.client().await;
    let workspace = client
        .call("workspace.create", json!({"name":"shutdown-proof"}))
        .await
        .unwrap();
    let pane = client
        .call(
            "pane.spawn",
            json!({"workspace_id":workspace["workspace"]["id"],"argv":["sleep","60"],"agent_hint":"codex"}),
        )
        .await
        .unwrap();
    let pane_id = pane["pane"]["id"].clone();
    client
        .call(
            "report-session",
            json!({"pane_id":pane_id,"agent":"codex","agent_session_id":"session-before-shutdown","resume_argv":["codex","resume","session-before-shutdown"]}),
        )
        .await
        .unwrap();
    emit(&service, true).await;
    released(&mut peer).await;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(server.state_dir.join("snapshot.json")).unwrap())
            .unwrap();
    let saved = snapshot["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == pane_id)
        .unwrap();
    assert_eq!(
        saved["agent"]["agent_session_id"],
        "session-before-shutdown"
    );
    assert_eq!(
        saved["agent"]["resume_argv"],
        json!(["codex", "resume", "session-before-shutdown"])
    );
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn harness_default_does_not_inhibit_even_when_a_bus_is_reachable() {
    let bus = PrivateBus::new();
    let (_service, lock) = bus.manager(false).await;
    let server =
        TestServer::start_with_env(&[("DBUS_SYSTEM_BUS_ADDRESS", bus.address.trim())]).await;
    let acquired = tokio::time::timeout(Duration::from_millis(400), async {
        loop {
            if lock.lock().unwrap().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        acquired.is_err(),
        "SIGNALTTY_LOGIND=0 still took an inhibitor"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn unavailable_logind_does_not_block_startup_and_is_retried() {
    let bus = PrivateBus::new();
    let mut server = bus.server().await;
    // Let the initial setup fail before making the service available.
    tokio::time::sleep(Duration::from_millis(100)).await;
    server
        .client()
        .await
        .call("server.status", json!({}))
        .await
        .unwrap();
    let (service, lock) = bus.manager(false).await;
    let mut peer = inhibitor(&lock).await;
    emit(&service, true).await;
    released(&mut peer).await;
    assert!(server.wait_for_exit(DEADLINE).await);
}

#[tokio::test]
async fn false_shutdown_signal_keeps_the_server_and_a_later_true_stops_it() {
    let bus = PrivateBus::new();
    let (service, lock) = bus.manager(false).await;
    let mut server = bus.server().await;
    let mut peer = inhibitor(&lock).await;
    emit(&service, false).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    match peer.read(&mut [0]) {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        other => panic!("false signal released the inhibitor: {other:?}"),
    }
    server
        .client()
        .await
        .call("server.status", json!({}))
        .await
        .unwrap();
    emit(&service, true).await;
    released(&mut peer).await;
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn initial_shutdown_property_stops_without_a_signal() {
    let bus = PrivateBus::new();
    let mut server = bus.server().await;
    let (_service, lock) = bus.manager(true).await;
    let mut peer = inhibitor(&lock).await;
    released(&mut peer).await;
    assert!(
        server.state_dir.join("snapshot.json").is_file(),
        "snapshot written while shutting down"
    );
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn denied_inhibit_is_retried_then_shutdown_saves() {
    let bus = PrivateBus::new();
    let (service, lock) = bus.manager_denying(false, 1).await;
    let mut server = bus.server().await;
    server
        .client()
        .await
        .call("server.status", json!({}))
        .await
        .unwrap();
    let mut peer = inhibitor(&lock).await;
    emit(&service, true).await;
    released(&mut peer).await;
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn logind_restart_releases_the_old_inhibitor_and_reconnects() {
    let bus = PrivateBus::new();
    let (service, lock) = bus.manager(false).await;
    let mut server = bus.server().await;
    let mut peer = inhibitor(&lock).await;
    drop(service);
    released(&mut peer).await;
    let (service, lock) = bus.manager(false).await;
    let mut peer = inhibitor(&lock).await;
    emit(&service, true).await;
    released(&mut peer).await;
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn server_shutdown_releases_the_inhibitor() {
    let bus = PrivateBus::new();
    let (_service, lock) = bus.manager(false).await;
    let mut server = bus.server().await;
    let mut peer = inhibitor(&lock).await;
    let _ = tokio::time::timeout(DEADLINE, async {
        server
            .client()
            .await
            .call("server.shutdown", json!({"force": true}))
            .await
    })
    .await;
    released(&mut peer).await;
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

#[tokio::test]
async fn snapshot_failure_still_releases_the_inhibitor_and_stops() {
    let bus = PrivateBus::new();
    let (service, lock) = bus.manager(false).await;
    let mut server = bus.server().await;
    let mut peer = inhibitor(&lock).await;
    let _restore = RestoreMode(server.state_dir.clone());
    let mut perms = std::fs::metadata(&server.state_dir).unwrap().permissions();
    perms.set_mode(0o500);
    std::fs::set_permissions(&server.state_dir, perms).unwrap();
    emit(&service, true).await;
    released(&mut peer).await;
    assert!(
        !server.state_dir.join("snapshot.json").exists(),
        "unwritable state dir must fail the save"
    );
    assert!(server.wait_for_exit(DEADLINE).await, "server shut down");
}

/// Puts the state directory back so test cleanup can remove it.
struct RestoreMode(std::path::PathBuf);

impl Drop for RestoreMode {
    fn drop(&mut self) {
        let Ok(meta) = std::fs::metadata(&self.0) else {
            return;
        };
        let mut perms = meta.permissions();
        perms.set_mode(0o700);
        let _ = std::fs::set_permissions(&self.0, perms);
    }
}
