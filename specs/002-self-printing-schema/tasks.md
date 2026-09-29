# Tasks: Self-Printing Schema

**Input**: Design documents from `/specs/002-self-printing-schema/`

**Prerequisites**: plan.md, spec.md

**Tests**: Required by constitution IV and AGENTS.md (new method →
integration test).

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Canonical lists in proto (US1+US2 foundation)

- [x] T001 [US1] Add failing test in
  `crates/signaltty-proto/src/lib.rs`: `ALL` slices contain every
  named constant, no duplicates (write `ALL` first as empty to watch
  it fail, or assert on a member before adding the slice)
- [x] T002 [US1] Add `method::ALL`, `event::ALL`, `code::ALL` +
  `method::SERVER_SCHEMA` in `crates/signaltty-proto/src/lib.rs`;
  `cargo test -p signaltty-proto`

## Phase 2: Server handler (US1)

- [x] T003 [US1] Add `h_server_schema` + dispatch arm in
  `crates/signaltty-server/src/router.rs` built from the `ALL`
  slices; `cargo test -p signaltty-server`

## Phase 3: Sync test (US2)

- [x] T004 [US2] Add `schema_lists_every_dispatched_method` to
  `crates/signaltty-server/tests/integration.rs`: call
  `server.schema`, probe every method except `server.shutdown` with
  `{}`, assert no `UNKNOWN_METHOD`; assert key members present.
  `cargo test -p signaltty-server --test integration schema_`

## Phase 4: CLI + docs (US1)

- [x] T005 [P] [US1] Add `Schema` variant + match arm in
  `crates/signaltty-cli/src/main.rs` (status-command pattern);
  verify with a live `signaltty schema` / `schema --json` run
- [x] T006 [P] [US1] Add `server.schema` row to the methods table in
  `docs/08-ipc.md`

## Phase 5: Verification

- [x] T007 Run `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`, `cargo test --workspace`

## Dependencies & Execution Order

- T001 → T002 → T003 → T004 → (T005, T006 in parallel) → T007

## Notes

- `server.shutdown` is listed in the schema but excluded from the
  probe loop: with `{}` on an idle store it would stop the fixture
  server. Its dispatch is covered by existing shutdown tests.
- Probing `workspace.create` with `{}` creates one empty workspace in
  the ephemeral fixture — harmless, assertion is dispatch-only.
- Commit as `server: self-printing schema (server.schema + CLI)` after
  T007 (CLI + docs ride along; one concern: the contract).
