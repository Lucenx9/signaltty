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
    fn configure(&mut self, opts: &TermOptions);       // font, theme, bell…
    fn destroy(&mut self, id: PaneId);
}
```

Implementations: `HeadlessBackend` (Phase 1, `vt100`, in server),
`VteBackend` (Phase 3, GUI), `GhosttyVtBackend` (future, feature-gated).
