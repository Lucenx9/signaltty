//! Permanent perf harness for the rendered line ring.
//! Run: `cargo run --release -p signaltty-term --example ring_perf`
//! Documented in docs/05-terminal-backend.md ("Performance" section).
//!
//! Scenarios mirror the orchestrator's targets: bulk 1 MiB feed,
//! shell-like 3k short lines, TUI repaint of one row 3k times.
//! Prints per-scenario wall time plus the vt100-only floor
//! (parser.process with no ring work) so regressions can be told apart
//! from vt100 crate costs.

use std::time::{Duration, Instant};

use signaltty_term::{HeadlessBackend, TerminalBackend};

fn feed_timed(b: &mut HeadlessBackend, id: &str, data: &[u8], repeats: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..repeats {
        b.feed_output(id, data);
    }
    start.elapsed()
}

fn vt100_floor(data: &[u8], repeats: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..repeats {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(data);
        std::hint::black_box(p.screen().contents());
    }
    start.elapsed()
}

fn scenario(name: &str, payload: Vec<u8>, target: Duration) {
    // Fresh surface per measurement so history depth is identical.
    let mut b = HeadlessBackend::new();
    b.create_surface("p", 80, 24);
    let dt = feed_timed(&mut b, "p", &payload, 1);
    std::hint::black_box(b.tail("p", 5, false));
    let floor = vt100_floor(&payload, 1);
    let status = if dt <= target { "OK  " } else { "MISS" };
    println!("{status} {name:<28} ring={dt:>9.3?} vt100-floor={floor:>9.3?} target={target:.0?}");
}

fn main() {
    // Bulk: 1 MiB of ~80-col lines in a single feed() call.
    let mut bulk = Vec::with_capacity(1024 * 1024);
    let mut i = 0u32;
    while bulk.len() < 1024 * 1024 {
        bulk.extend_from_slice(format!("bulk line {i:08} {:*<60}\r\n", "").as_bytes());
        i += 1;
    }
    bulk.truncate(1024 * 1024);

    // Shell-like: 3000 short lines in one feed.
    let mut shell = Vec::new();
    for k in 0..3000 {
        shell.extend_from_slice(format!("line {k} ok\r\n").as_bytes());
    }

    // TUI repaint: one spinner row rewritten 3000 times (CR, no LF).
    let mut repaint = Vec::new();
    for k in 0..3000 {
        repaint.extend_from_slice(format!("spin {k}\r").as_bytes());
    }

    println!("ring_perf (release), 80x24 surface, single feed() per scenario:");
    scenario("bulk 1MiB", bulk, Duration::from_millis(30));
    scenario("shell 3k lines", shell, Duration::from_millis(10));
    scenario("TUI repaint 3k", repaint, Duration::from_millis(10));
}
