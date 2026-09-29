# Feature Specification: Agent Skill + Named Handles + Audit Log

**Feature Branch**: `006-agent-skill-handles-audit`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "docs/14 directive 4 (server owns sessions;
herdr named sessions, cmux cli-contract discipline) and directive 5
(agents are API clients: HERDR_ENV-style gated skill, wait primitive)
are half-done: no shippable skill file, workspaces have non-unique names
but no typable handles, and `subscribe {from_seq}` replays only a 1024
in-memory ring (no JSONL audit)."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Agents read the skill, gated to managed panes (P1)

An agent wakes inside a managed pane (`SIGNALTTY_PANE` set), reads one
file, and knows the contract (schema first), its tools, and the rules
(act only here, wait don't poll, never fake decisions). Outside a pane
the skill refuses (`skill check` exits 1) so automations can't mistake a
bare shell for a managed one.

**Independent Test**: `signaltty skill` prints the embedded doc;
`skill check` exits 0/1 with/without `SIGNALTTY_PANE`; `skill install
--home TMP` writes only our marked files into the three harness dirs.

**Acceptance Scenarios**:

1. **Given** `SIGNALTTY_PANE=pane_x`, **When** the agent runs `skill
   check`, **Then** exit 0. Without the env, exit 1 with a reason.
2. **Given** `skill install --home TMP`, **When** run twice, **Then**
   idempotent; `uninstall` removes only marked files, keeps user files.

### User Story 2 - Workspaces have typable handles (P2)

`signaltty new --name "my api"` prints `my-api`; every `workspace_id`
param accepts handle-or-id; `workspace list` shows both. Handles are
immutable, unique, URL/slug-safe.

**Independent Test**: create two workspaces with the same name → handles
`my-api`, `my-api-2`; `workspace.get` by handle works; old snapshots
without handles load with backfilled unique handles.

**Acceptance Scenarios**:

1. **Given** name `"My API!!"`, **When** created, **Then** handle
   `my-api`.
2. **Given** handle `my-api`, **When** passed as `workspace_id` to
   `workspace.get/rename/close/refresh_git`, `tab.create`, `pane.spawn`,
   **Then** resolved like the id.
3. **Given** a pre-handle snapshot, **When** loaded, **Then** every
   workspace owns a unique handle (migration, tested).

### User Story 3 - Events survive in a JSONL audit (P3)

Every broadcast event (except high-volume `pty.data`) appends to
`state_dir/audit.jsonl`; `subscribe {from_seq}` backfills from the file
when the ring has rotated; rotation caps disk use.

**Independent Test**: emit > ring capacity via rapid notifications… (ring
is 1024; cheaper: restart server → ring empty → subscribe from old seq
still replays from file).

**Acceptance Scenarios**:

1. **Given** events emitted then server restarted, **When** a client
   subscribes with the old `from_seq`, **Then** it receives the audited
   events (backfill), not an empty replay.
2. **Given** an 8MB+ audit file, **When** appending, **Then** rotation
   keeps `audit.jsonl` + one `.1` (bounded disk, tested with small cap
   via unit test on the rotation helper).

## Requirements *(mandatory)*

- **FR-001**: Skill text lives in one source (`SKILL.md` asset, embedded
  with `include_str!`); `skill`, `skill check/install/uninstall/status`
  are the only surface. Install marker: `<!-- signaltty-skill -->`.
- **FR-002**: `Workspace.handle`: slug (lowercase alnum + `-`, ≤32 chars,
  `ws` fallback), unique per server, immutable after create. Resolver:
  id first, then handle (ids stay unambiguous — handles never collide
  with `ws_` prefix… enforce: handles must not contain `_`).
- **FR-003**: Audit appends `{seq, event, payload, at}` per `Ctx::emit`
  (skip nothing — `pty.data` never passes through `emit`); fsync
  discipline = same as snapshot (no fsync per event; OS crash may lose
  the tail — documented, matches snapshot honesty).
- **FR-004**: Replay merges audit-backfill + ring, dedupes by seq, caps
  at 2048 events per subscribe.
- **FR-005**: Per-session sockets are explicitly deferred (ADR): one
  socket + named handles cover the workflows; multi-socket would double
  the auth/snapshot surface for no proven demand.

## Success Criteria *(mandatory)*

- **SC-001**: Fresh agent onboards from `skill` + `server.schema` alone
  (reviewed by reading, no code test).
- **SC-002**: Handle collision/legacy-migration tests green.
- **SC-003**: Restart-replay test green; disk bounded by rotation test.
