# 05 — Terminal Backend Research (2026-09)

Question: what renders terminal state in the GUI, and what parses it
headless in the server? Researched upstream state; no APIs invented.

## libghostty / libghostty-vt (Ghostty)

- **Shipped: `libghostty-vt` only.** Zero-dependency (not even libc)
  C/Zig library: VT sequence parsing, terminal grid state, cursor,
  scrollback + resize reflow, selection internals, key/mouse input
  encoding. Builds for Linux/macOS/Windows/WASM. Rust linking proven
  possible (only libc/libm deps).
- **API explicitly unstable.** Header warns "incomplete,
  work-in-progress". Breaking change observed between 2026-07-20
  (`GhosttyTerminalOptions` struct) and 2026-08-06 (geometry args);
  forward-compat via `GHOSTTY_INIT_SIZED` sized-struct pattern.
  Consumers must pin a commit and expect churn.
- **No renderer.** `libghostty-vt` deliberately excludes font shaping,
  GPU rendering, and widgets. Full `libghostty` (rendering + config +
  `apprt` runtimes) is announced ("libghostty is coming") but not
  shipped as a stable embeddable API.
- **GTK frontend is in-tree Zig**, calling the GTK4 C API from
  `src/apprt/gtk/`. Reusable only by copying Ghostty internals —
  fragile, rejected.

Conclusion: do not build the GUI on Ghostty today. Track
`libghostty-vt` behind the `TerminalBackend` trait as a future
headless-parser option; do not copy private internals.

## Rust VT/parser crates (server-side headless model)

| Crate | On crates.io | Notes |
|---|---|---|
| `portable-pty` | yes | PTY spawn/resize/kill. De-facto standard (WezTerm family). **Chosen for PTY.** |
| `vt100` | yes | Small screen-model parser, MSRV-friendly, ratatui-friendly. Loses some attrs. Good enough for snapshot/OSC fallback. **Chosen for Phase 1 headless model.** |
| `alacritty_terminal` | yes | Fuller emulation, heavier deps, tracks Alacritty releases, no stability promise. Fallback if `vt100` proves too lossy. |
| `wezterm-term` | **no** (git-only) | Best API for mux use (WezTerm's own), but unpublished; `tattoy-wezterm-term` is a third-party republish. Rejected: no crates.io = no publishable builds, API declared unstable. |
| `vte` | yes | Parser only (no state). Useful for a strict OSC extractor if needed. |

Decision: `portable-pty` + `vt100` for Phase 1; OSC 9/99/777 extracted
by a dedicated scanner on the raw byte stream (not dependent on the
screen parser), with `vte` as the strict fallback.

## GUI terminal widget options

- **VTE (`vte4` via gtk4-rs)**: mature GNOME terminal engine, GTK4 +
  Wayland native, handles PTY itself — but here the PTY lives in the
  server, so VTE would be used as a *dumb renderer fed by the server
  stream*. Feeding VTE externally (via `feed`) while it owns no child
  is supported and keeps us on a battle-tested renderer with selection,
  accessibility, IME, and font handling for free. Mainstream choice of
  GTK4 Rust apps (relm4 + gtk4 + vte4 pattern is proven).
- **Custom GPU renderer** (glyph atlas + `libghostty-vt` state): maximum
  control, maximum work (fonts, shaping, IME, a11y all hand-rolled).
  Revisit only if VTE-as-renderer proves inadequate.
- **Embedding Ghostty's GTK runtime**: rejected (Zig in-tree, no C API).

## Abstraction

All GUI code talks to this trait, never to a concrete engine:

```rust
trait TerminalBackend {
    fn create_surface(&mut self, id: PaneId, cols: u16, rows: u16);
    fn feed_output(&mut self, id: PaneId, data: &[u8]);
    fn send_input(&mut self, id: PaneId, data: &[u8]); // → server IPC
    fn resize(&mut self, id: PaneId, cols: u16, rows: u16);
    fn snapshot(&self, id: PaneId) -> String;          // plain-text screen
    fn screen_state(&self, id: PaneId) -> Vec<u8>;     // replayable VT bytes
    fn configure(&mut self, opts: &TermOptions);       // font, theme, bell…
    fn destroy(&mut self, id: PaneId);
}
```

Implementations: `HeadlessBackend` (Phase 1, `vt100`, in server),
`VteBackend` (Phase 3, GUI), `GhosttyVtBackend` (future, feature-gated).

## Rendered ring (US3 reads, 018)

`HeadlessBackend` keeps one rendered line ring per pane
(`crates/signaltty-term/src/headless.rs`), built from the RENDERED grid —
never from raw `\n` splitting, which garbles full-screen TUIs (cursor
moves, CR overwrites and repaints concatenate instead of resolving).
Capacity is 5000 content lines per pane; every line carries a monotonic
`content_seq` (runtime-only, reset on restart, so pre-restart cursors read
as dropped). `tail` is rebased on the ring (same `{text, truncated}`
shape); `pane.read` `mode: "rendered"` adds the cursor
(`after_seq`/`lines` → `text`/`seq`/`next_seq`/`dropped`/`truncated`, see
docs/08). Reads are passive and never refused on agent state.

How it works: `feed()` splits PTY bytes into small batches (cut after
every CR/LF, plus a ~one-visual-row byte budget; splitting is UTF-8 safe
and harmless to the `vt100` state machine) and reconciles the ring after
each batch. Scroll detection matches the surviving grid prefix against the
ring with the tail overlap excused (a print that scrolls in the same step
lands new rows there), bounded by the batch's own scroll potential
(newlines + scroll controls + printable rows + slack) so repetitive
content cannot match a wild shift; among matches the largest shift with
non-blank evidence wins, else the smallest. Changed rows keep their
position but take a fresh sequence, so a repaint never grows the ring and
a polling cursor observes each repaint exactly once. Trailing blank grid
rows are padding, not content (matches `contents()` trimming).

Alt-screen decision: while a TUI owns the alternate grid there is no
scrollback (engine limit — `vt100` keeps a separate zero-scrollback grid,
and the ring stores one screen only). Reads return the current alt grid;
repaints bump touched rows' sequences, so an orchestrator polling with
`next_seq` gets deltas without duplicates; nothing is appended to history
on repaint or on alt enter/exit, and exiting restores the main grid
verbatim. Deliberately no herdr-style `agent_not_idle` refusal: our reads
are passive and side-effect-free, so refusal would buy nothing.

Known limits (documented, not silent): explicit clears (`ESC[2J`)
preserve the cleared lines into history rather than dropping them; wrapped
continuation rows read as separate lines (no wrap-joining); a bare `\n`
without `\r` staircases exactly like a real terminal (PTYs in cooked mode
deliver `\r\n` via ONLCR, so shell output is unaffected); restores
re-sequence from 1 with a dropped-floor, so every pre-restart cursor
reads as dropped (Phase 7's cursor-reset line must keep this: see
`dropped_floor`).

Performance: `feed()` walks PTY bytes on the legacy cut points
(CR/LF + one-visual-row budget) and classifies each batch. Plain-ASCII
batches in Ground state fuse into fast spans that are captured from the
input side with zero grid reads; everything else reconciles from the
grid exactly as before, so every legacy reconcile observes the same grid
state at the same byte offset as the pre-fast-path code.

The fast row simulation mirrors vt100 for ASCII (width-1 cells, wrap at
`col == cols`, CR resets col, LF/VT/FF scroll at the bottom margin) and
commits only after the simulated end cursor matches the parser's real
cursor; any mismatch falls back to grid truth, and debug builds assert
full grid sync after every feed. Groundness, partial UTF-8 (exact
positional validity per utf8parse, not a counter) and the two sticky
vt100 modes that affect plain layout (scroll region `CSI r`, origin mode
`CSI ? 6 h` — vt100 ignores LNM/IRM/wrap-suppression/charsets, so those
need no tracking) are scanned left to right with carry across feeds.
Debug `cargo test` therefore cross-checks the fast path continuously:
the fuzzer-hostile cases (repetitive streams, blank scrolls, split
sequences, sticky modes, chunked PTY fragmentation) are pinned in
`headless.rs` tests.

Measured `feed()` throughput (release, 80×24, `cargo run --release -p
signaltty-term --example ring_perf`): bulk 1 MiB ≈ 22 ms (vt100 floor
≈ 13 ms; was ≈ 1.1 s reconciling per batch), shell-like 3k lines
≈ 1.3 ms (floor ≈ 0.8 ms; was ≈ 63 ms), TUI repaint 3k ≈ 0.4 ms (floor
≈ 0.3 ms; was ≈ 12 ms). Bulk is ~1.6× the parse floor — the ring keeps
one String alloc per captured line (unavoidable: history owns the text)
plus one screen-shift memmove per scroll. A release-only regression gate
(`fast_ring_perf_regression`, ignored by default: `cargo test --release
-p signaltty-term -- --ignored fast_ring_perf`) fails only on an
algorithmic relapse toward per-batch reconciliation.

Two legacy-matcher quirks are intentionally not preserved (no test
pinned them; both were bugs): repetitive streams no longer duplicate
lines (30×`"y"` read back as 41), and a blank LF on a full screen moves
the top line to history instead of dropping it. vt100's own scrollback
buffer was evaluated and rejected: its public API exposes only a
screen-sized viewport with an offset, so scrolled-off rows are not
enumerable without O(history) paging per feed.
