# Implementation Plan: Agent Manifests + Live /proc Refresh

**Branch**: `005-agent-manifests-proc` | **Date**: 2026-09-29 | **Spec**: `specs/005-agent-manifests-proc/spec.md`

## Summary

Data-not-code detection: TOML overlays over the builtin adapters (extra
binaries, hook-map overrides, session key, resume template, display name),
loaded once at startup from an isolated-failure dir scan. Plus the promised
docs/07 layer 4: a 10s proc tick promoting kind and following cwd, emitting
only on change.

## Technical Context

**Language/Version**: Rust 1.85, edition 2021
**Primary Dependencies**: `toml = "0.8"` (already used by `signaltty-plugin`;
add to `signaltty-agent`), tokio interval (existing pattern in `server.rs`)
**Storage**: N/A (manifests are config, not state; proc data is ephemeral)
**Testing**: agent in-module unit tests (pure parse + overlay); server
integration via testkit (manifest dir override, `sleep` promotion, cwd
follow, malformed isolation)
**Target Platform**: Linux (`/proc`; other platforms: tick is a no-op —
code paths already Linux-only via `attrib.rs`)
**Performance Goals**: tick scans `/proc` once per 10s; zero events and
zero persist marks when nothing changed
**Constraints**: taxonomy stays a closed `AgentKind`; overlays never widen
it. No OS calls in `signaltty-agent` (parse pure; file reads in server).
`signaltty-core` untouched.

## Constitution Check

- I: spec first (this branch), then tasks, then code.
- V: parse seam `agent::parse_manifest(&str)`; overlay seam
  `OverlayAdapter: AgentAdapter` (concrete type, server holds a `Vec`);
  routing via one `Ctx::adapter()` helper so emits stay paired through
  existing `Store` methods. New `pane.updated` emissions reuse the
  existing event (no new event types — subtract before you add).
- VII: no new IPC methods (status gains a field), no daemon, no polling
  faster than 10s, no demotion logic.

## Project Structure

```text
crates/signaltty-agent/src/manifest.rs   # Manifest, parse_manifest, OverlayAdapter
crates/signaltty-agent/src/lib.rs        # re-exports
crates/signaltty-agent/Cargo.toml        # + toml dep
crates/signaltty-core/src/paths.rs       # agents_dir()
crates/signaltty-server/src/config.rs    # agents_dir field
crates/signaltty-server/src/router.rs    # Ctx::adapter(), overlays in Ctx, detect at spawn
crates/signaltty-server/src/server.rs    # load overlays at startup, 10s procscan tick
crates/signaltty-server/src/procscan.rs  # NEW: /proc deepest-descendant + cwd, apply-if-changed
crates/signaltty-server/tests/integration.rs
crates/signaltty-cli/src/integration.rs  # status shows manifests
docs/07-agents.md (manifest section) + docs/adr/0009-agent-manifests.md
```

**Structure Decision**: overlay lookup lives on server `Ctx` (no globals,
testable); agent crate stays OS-free.

## Design

### Manifest schema (`agents/<name>.toml`)

```toml
[agent]
kind = "codex"                 # required, existing AgentKind
display_name = "Codex (wrap)"  # optional
binaries = ["codex-wrap"]      # merged with builtin binaries

[session]
key = "session_id"             # payload key override (optional)
resume = ["codex", "resume", "{session_id}"]  # template override (optional)

[lifecycle.SomeFutureHook]     # any hook name; unknown hooks fall through
lifecycle = "working"          # optional, Lifecycle::parse
attention = "unread"           # optional, Attention::parse
message = "…"                  # optional
```

Unknown top-level/hook fields ignored (forward compat). Bad `kind` /
bad enum strings → whole file rejected with a logged error (fail loud
per file, isolated across files).

### Overlay resolution

```rust
struct OverlayAdapter { builtin_kind: AgentKind, manifest: Manifest }
impl AgentAdapter for OverlayAdapter {
    identify: builtin.identify(proc) || manifest.binaries.contains(bin_name)
    lifecycle_state: overlay map hit (each field optional; missing fields
        fall through to builtin decision field-by-field) else builtin
    session_identity: payload[manifest.session.key] or builtin
    notification_event: builtin
    resume_capability: template (replace {session_id}) or builtin
    answer_channel: builtin
    metadata: kind builtin, display_name override or builtin, binaries merged
}
```

`Ctx::adapter(&self, name: &str) -> &dyn AgentAdapter`: first overlay whose
`kind` matches `AgentKind::parse(name)`, else `adapter_for_name` (unknown →
caller `BAD_PARAMS` as today). Spawn detection: `detect_kind` gains an
overlay-binaries pass — implement as `agent::detect_kind_with(argv,
extra: &[(&AgentKind, &[String])])`? Simpler: `Ctx::detect_kind(&self,
argv)` iterating overlays then builtins. Hmm — keep `detect_kind` pure and
add `detect_kind_with_overlays(argv, overlays)`. Fine.

### Proc tick (`procscan.rs`)

```rust
pub fn deepest_descendant(child_pid: u32) -> u32  // youngest /proc descendant, shell-transparent
pub fn proc_argv0(pid: u32) -> Option<String>     // /proc/<pid>/cmdline first field basename
pub fn proc_cwd(pid: u32) -> Option<String>       // /proc/<pid>/cwd symlink
pub fn scan(store, child_pids: &HashMap<String, u32>) -> Vec<StoredEvent>
  // per live pane: bin = argv0(deepest(child)); if kind is Generic/None and
  //   bin matches overlay-or-builtin binaries → promote; cwd refresh on change.
  //   Emits pane.updated per changed pane (single existing event type).
```

Tick wiring in `server.rs`: `tokio::spawn` 10s interval calling
`procscan::scan` under one store lock, broadcast + `mark_persist` only
when non-empty. Shell transparency: if direct child's basename is a known
shell (`sh|bash|zsh|fish|dash`), descend; else use the child itself. Only
ONE level of shell skip + youngest-child pick keeps it cheap and honest
(documented limitation, not a process-tree oracle).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| New `procscan` module | `/proc` traversal + change detection is one seam with pure-ish helpers testable against own pid | Inline in `server.rs` would tangle the accept loop with OS parsing |
| `Ctx::adapter` indirection | Overlay-first routing in hook-event/report-session/answer without globals | Global OnceLock in agent crate breaks test isolation and puts OS config in a pure crate |
