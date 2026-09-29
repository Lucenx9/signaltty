//! Opt-in IPC/event counters for refresh measurements. No payloads are logged.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

pub fn record(kind: &str, name: &str) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    static IPC: AtomicU64 = AtomicU64::new(0);
    static EVENTS: AtomicU64 = AtomicU64::new(0);
    if !*ENABLED.get_or_init(|| std::env::var_os("SIGNALTTY_REFRESH_METRICS").is_some()) {
        return;
    }
    let counter = if kind == "ipc" { &IPC } else { &EVENTS };
    let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
    eprintln!("[refresh-metrics] {kind}={count} name={name}");
}
