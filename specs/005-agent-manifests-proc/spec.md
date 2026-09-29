# Feature Specification: Agent Manifests + Live /proc Refresh

**Feature Branch**: `005-agent-manifests-proc`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "docs/14 directive 3 requires Know-thy-agents
two ways — hooks plus declarative per-agent detection manifests (herdr
`manifests/*.toml`: data, not code). Today every adapter is hardcoded Rust,
and spawn-time argv is the only process signal (no live /proc refresh even
though docs/02, docs/07 promise cwd/kind tracking)."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Extend detection without recompiling (Priority: P1)

A user runs agents through wrapper scripts (`codex-wrap`) or needs a new
hook mapping after a CLI update. They drop one TOML file into the agents
dir and the server picks it up on restart (and `integration status` shows
it) — no Rust change, no rebuild.

**Why this priority**: Directive 3 verbatim — "data, not code,
community-extensible". Hardcoded adapters mean every CLI churn is a
code change.

**Independent Test**: manifest mapping `sleep`→codex binaries in an
isolated agents dir; `hook-event --agent codex` from that binary classifies
as codex; malformed manifest logged + skipped, server still starts.

**Acceptance Scenarios**:

1. **Given** `agents/codex-extra.toml` with `binaries = ["codex-wrap"]`,
   **When** a pane spawns `codex-wrap`, **Then** its kind is `codex`.
2. **Given** a manifest with `[lifecycle.SomeFutureHook] lifecycle =
   "working"`, **When** that hook arrives, **Then** lifecycle is working
   (builtin fallthrough for unmapped hooks is unchanged).
3. **Given** a malformed manifest, **When** the server starts, **Then** it
   logs, skips that file, and serves everything else.

### User Story 2 - Panes track the live process (Priority: P2)

A pane spawns `sh` which execs `codex` (or the user runs another agent
inside a shell). Within seconds the sidebar shows the real agent and the
real cwd — without hooks, without restart.

**Why this priority**: Docs/07 layer 4 (foreground process info) and
docs/02 (cwd refreshed from `/proc`) are promised but unimplemented;
spawn argv alone mislabels every shell-hosted agent.

**Independent Test**: spawn `sh -c 'sleep 30'` as generic; proc tick
keeps kind generic but refreshes cwd; spawn with manifest mapping
`sleep`→codex; tick promotes kind to codex and emits `pane.updated`
once (quiet afterwards).

**Acceptance Scenarios**:

1. **Given** a live generic pane whose deepest child is a manifested
   binary, **When** the proc tick runs, **Then** kind promotes (generic
   → specific only, never demotes) and one `pane.updated` emits.
2. **Given** a pane whose child cd's, **When** the tick runs, **Then**
   `pane.cwd` follows `/proc/<pid>/cwd` and emits on change only.
3. **Given** no change, **When** ticks run, **Then** no events, no
   persistence marks.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Manifests live in `paths::agents_dir()` (`$XDG_CONFIG_HOME/
  signaltty/agents/*.toml`, `SIGNALTTY_AGENTS_DIR` override); loaded once
  at server start; failures isolated per file.
- **FR-002**: Manifest `kind` MUST be an existing `AgentKind` (no new agent
  taxonomy without a Rust adapter — taxonomy stays typed).
- **FR-003**: Overlays merge: extra `binaries`, per-hook
  `{lifecycle?, attention?, message?}` overrides, `session.key` payload key,
  `resume` argv template with `{session_id}`, `display_name` override.
  Everything unmapped falls through to the builtin adapter.
- **FR-004**: Hook routing (`agent` name) and spawn detection consult
  overlays first; `detect_kind` takes the overlay binaries into account.
- **FR-005**: Proc tick every 10s over live panes only: deepest-descendant
  basename → kind promotion (generic/none → specific), `/proc` cwd →
  `pane.cwd`; emit `pane.updated` + persist only on change.
- **FR-006**: `integration status` reports loaded manifests (name, kind,
  file) alongside hook shims.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Manifest-only test (no Rust change) promotes a `sleep` pane
  to codex end-to-end over the socket.
- **SC-002**: A cwd change inside a live pane appears in `pane.get`
  within ~15s with exactly one `pane.updated`.
- **SC-003**: Malformed manifest never prevents startup (integration test
  with garbage file).
