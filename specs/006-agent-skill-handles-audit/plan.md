# Implementation Plan: Agent Skill + Named Handles + Audit Log

**Branch**: `006-agent-skill-handles-audit` | **Date**: 2026-09-29 | **Spec**: `specs/006-agent-skill-handles-audit/spec.md`

## Summary

Directive 5's substrate in three small slices: an embedded gated skill
doc with install plumbing; immutable unique workspace handles accepted
everywhere ids are; a JSONL audit backing `subscribe {from_seq}` past
ring rotation and restarts. Per-session sockets deferred by ADR.

## Technical Context

**Language/Version**: Rust 1.85, edition 2021
**Primary Dependencies**: none new (fs + serde_json only)
**Storage**: `state_dir/audit.jsonl` (+ one `.1` rotation); `handle` in
snapshot `Workspace` (migration fills legacy empties)
**Testing**: CLI tests (skill surface, install isolation); server
integration (handles, migration via crafted snapshot?, restart-replay);
`audit.rs` unit tests (rotation helper, merge-dedupe-cap)
**Target Platform**: Linux
**Performance Goals**: audit append = one buffered line write per event
(no fsync/event); replay backfill capped (scan ≤ 2 files, emit ≤ 2048)
**Constraints**: handles never contain `_` (id/handle namespaces disjoint
by construction); `Workspace` stays backward-loadable (`handle` defaults,
migration in `persist::apply`); `Ctx::emit` stays sync and cheap
**Scale/Scope**: 1 asset + 1 CLI command group, 1 model field + resolver,
1 server module + replay merge

## Constitution Check

- I: spec → plan → tasks → code.
- V: typed params unchanged (resolver sits behind existing `workspace_id`
  fields — shape identical, domain resolution widens); audit appends
  inside `Ctx::emit` so no emit escapes the log; replay merge is one
  helper with unit tests.
- VII: no new IPC methods (skill is CLI-local; handles reuse fields;
  audit reuses subscribe); no per-session sockets; rotation instead of
  retention config.

## Project Structure

```text
crates/signaltty-cli/assets/SKILL.md      # single source, include_str!
crates/signaltty-cli/src/main.rs          # `skill` command group
crates/signaltty-cli/src/skill.rs         # NEW: check/install/uninstall/status
crates/signaltty-cli/tests/cli.rs
crates/signaltty-core/src/model.rs        # Workspace.handle + slugify
crates/signaltty-server/src/router.rs     # handle derivation, resolve_workspace()
crates/signaltty-server/src/persist.rs    # migration backfill
crates/signaltty-server/src/audit.rs      # NEW: append/read_since/rotate
crates/signaltty-server/src/server.rs     # Ctx audit field, replay merge
crates/signaltty-server/tests/integration.rs
docs/08-ipc.md (handles + audit notes) + docs/adr/0010-deferred-sockets.md
```

## Design

### Skill (`skill.rs` + asset)

```rust
pub const TEXT: &str = include_str!("../assets/SKILL.md");  // marker-checked in test
pub fn check() -> Result<(), String>   // SIGNALTTY_PANE set + non-empty
pub fn install(home) / uninstall(home) // 3 harness dirs, marker `<!-- signaltty-skill -->`
pub fn status(home, json)
```

Harness dirs under home: `.agents/skills/signaltty/`,
`.claude/skills/signaltty/`, `.codex/skills/signaltty/`. Install writes
`SKILL.md` (marker head comment); uninstall removes the file only if the
marker is present, then prunes now-empty `signaltty/` dirs (never the
parent skills dir). Status reports per-dir installed/not + agents-dir? No —
manifests already covered in `integration status`.

### Handles

```rust
// core
pub fn slugify(name: &str) -> String  // lowercase alnum→self, rest→'-', collapse, trim, truncate 32, fallback "ws", never contains '_'
Workspace { handle: String }  // #[serde(default)] for legacy snapshots
```

Router: `unique_handle(store, base) -> String` (base, base-2…); set at
`workspace.create` (from name or "workspace"); `resolve_workspace(store,
handle_or_id) -> Option<id>`: exact id match first, else handle match.
Apply in get/rename/close/refresh_git/tab.create/pane.spawn. Rename keeps
handle immutable (documented: handle is the stable address, name the label).
`workspace.list/get` already serialize whole struct → handle visible; CLI
`new` prints handle; `workspace list` human line gains `handle=`.

Migration in `persist::apply`: workspaces with empty handle get
`unique_handle` over the loading set (deterministic: sorted by id).

### Audit (`audit.rs`)

```rust
pub struct AuditLog { path: PathBuf, file: Mutex<BufWriter<File>> }
impl AuditLog {
    pub fn open(state_dir) -> io::Result<AuditLog>   // create_dir_all + append open
    pub fn append(&self, seq: u64, name: &str, payload: &Value)  // one JSON line; rotate first if > CAP
    pub fn read_since(&self, from_seq: u64, limit: usize) -> Vec<StoredEvent>  // .1 then current, filter, cap
}
pub const CAP_BYTES: u64 = 8 * 1024 * 1024;
```

Rotation check per append via cached len (metadata refreshed per append —
one stat per event, cheap vs PTY bytes which never pass here). `Ctx::emit`
appends after broadcast-send (ordering: seq assigned in store first, so the
file order matches seq order).

Replay merge in `handle_conn`: `backfill = audit.read_since(from, 2048)`,
`ring = events_since(from)`, merge by seq (BTreeMap), take… all (both
capped; total ≤ 4096 — then truncate to the NEWEST 2048? Oldest 2048 from
`from`? Correct replay semantic: everything after `from`. Cap at 4096 with
a comment; realistic flows never hit it (1024 ring was fine).

Unit tests: rotation helper with tiny cap (tempfile? use temp dir +
`CAP` override via param — make cap a field, not const, for tests);
merge-dedupe test with fabricated seqs.

Integration: restart-replay — emit notification, read seq, shutdown
(graceful, snapshot persists anyway), respawn (testkit `respawn`? check:
testkit has respawn preserving state dir? `restart_restores_structure…`
test exists — reuse pattern), subscribe from_seq → receives
notification.created.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| `Workspace.handle` migration | Old snapshots must load (honest restart rule) | Refusing old snapshots breaks every existing user |
| Replay merge helper | Ring + file overlap after rotation/restart | File-only replay would re-read MBs per subscribe |
