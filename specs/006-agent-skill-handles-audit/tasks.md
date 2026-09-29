# Tasks: Agent Skill + Named Handles + Audit Log

**Input**: `spec.md` + `plan.md` (this dir)

**Seams under test**: CLI skill surface; `slugify` + resolver;
`persist::apply` migration; `AuditLog` append/read/rotate;
socket restart-replay.

## Phase 1: US1 - Gated agent skill (P1)

 - [x] T001 Skill asset + `skill.rs`: `SKILL.md` (~80 lines: identity,
  schema-first, tools, rules, examples; marker head), `check`,
  `install/uninstall/status`, `TEXT` marker test.
 - [x] T002 CLI `skill` group wiring in `main.rs` + tests in `tests/cli.rs`
  (print, check in/out of pane, install idempotent + isolation).

## Phase 2: US2 - Named handles (P2)

 - [x] T003 Core `slugify` + `Workspace.handle` (serde default) + unit tests.
 - [x] T004 Router: derive-at-create, `resolve_workspace`, widen
  get/rename/close/refresh_git/tab.create/pane.spawn; `persist::apply`
  migration; CLI prints handle (`new`, `workspace list`).
 - [x] T005 Integration: collision (`my-api`, `my-api-2`), handle-or-id
  everywhere, legacy snapshot migration (craft snapshot.json without
  handles into state dir pre-start, assert backfilled).

## Phase 3: US3 - JSONL audit (P3)

 - [x] T006 `audit.rs`: open/append/read_since/rotate + unit tests
  (rotation with tiny cap, merge order).
 - [x] T007 Wire `Ctx::emit` append + replay merge in `handle_conn`
  (cap 4096, comment).
 - [x] T008 Integration: restart-replay (notify → seq → shutdown →
  respawn → subscribe from_seq → event received).

## Phase 4: Polish

 - [x] T009 Docs: `docs/08-ipc.md` (handles accepted as `workspace_id`,
  audit file + replay semantics) + `docs/adr/0010-deferred-sockets.md`.
 - [x] T010 Gates: fmt, clippy (zero new), full tests; commits.
