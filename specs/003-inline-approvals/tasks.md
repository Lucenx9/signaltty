# Tasks: Inline Approvals

**Input**: `spec.md` + `plan.md` (this dir)

**Seams under test** (agreed): core model serde at `Pane`/`Decision`;
proto constant lists; `params::decode`; `Store` transitions;
router methods over socket (`decision.answer`, `hook-event`+decision);
`answer_bytes` pure fn; adapter `answer_channel`; CLI arg mapping;
`decision_render` pure fn in GUI.

## Phase 1: Setup

 - [x] T001 Confirm `codex` shim event names used by fixtures
  (`PermissionRequest`, `UserPromptSubmit`, `Stop`) in
  `crates/signaltty-agent/src/adapters/codex.rs` — read-only.

## Phase 2: Foundational (blocks all stories)

 - [x] T002 [US1/US2] Core `Decision`/`DecisionOption` + `Pane::pending_decision`
  in `crates/signaltty-core/src/model.rs` (RED: serde roundtrip, `None`
  default for old snapshots; GREEN: minimal structs).
 - [x] T003 [US1/US2] Proto contract in `crates/signaltty-proto/src/lib.rs`:
  `DECISION_ANSWER`, `DECISION_CREATED/ANSWERED/CLEARED`,
  `NO_SUCH_DECISION`; update `ALL` lists + count asserts (33→34, 25→28,
  14→15).
 - [x] T004 [US1/US2] `AnswerChannel::TypeText` + `answer_bytes` pure fn +
  `AgentAdapter::answer_channel` default `None` in
  `crates/signaltty-agent/src/types.rs` (RED: bytes test; GREEN).
 - [x] T005 [US2] Params: `HookEvent.decision: Option<DecisionPayload>`
  (unknown fields ignored, mistyped → `BAD_PARAMS`) + `DecisionAnswer`
  struct in `crates/signaltty-server/src/params.rs` with decode tests.
 - [x] T006 [US1/US2] Store transitions in `crates/signaltty-server/src/store.rs`:
  `set_decision` (created, supersede folds into one emit with `prev`),
  `answer_decision` (answered or `None` on id mismatch),
  `clear_decision` (cleared or `None` when absent) + unit tests.

## Phase 3: US1 - Answer where you look (P1) 🎯 MVP

**Independent Test**: fixture pane with decision → `decision.answer`
delivers bytes to PTY, consumes id, emits `answered`; bar clears on
attention clear.

 - [x] T007 [US1] Router: decision ingest in `h_hook_event`
  (`crates/signaltty-server/src/router.rs`) — set/supersede on
  `decision` param; clear on lifecycle-leaves-Blocked/attention-cleared.
 - [x] T008 [US1] Router: `h_decision_answer` — id checks, channel check,
  `ptys.input` delivery, consume+emit; double/stale → `{answered:false}`
  ok; unknown option → `BAD_PARAMS`; exited → `PANE_EXITED`.
 - [x] T009 [US1] Integration tests in
  `crates/signaltty-server/tests/integration.rs`: ingest→get→answer
  byte-delivery via `cat` pane; double-answer no-redelivery;
  prose-only keeps no decision; `UserPromptSubmit` clears decision.
 - [x] T010 [US1] Codex `answer_channel() -> Some(TypeText)` in
  `crates/signaltty-agent/src/adapters/codex.rs` + unit test
  (others keep default; assert `None`).

## Phase 4: US2 - Structured requests as data (P2)

**Independent Test**: `hook-event` with decision → `pane.get` returns it;
prose-only → no decision object anywhere.

 - [x] T011 [US2] CLI: `hook-event --decision '<json>'` + `decision answer`
  subcommand + decision lines in `pane get` human output
  (`crates/signaltty-cli/src/main.rs`).
 - [x] T012 [US2] GUI decision bar in `crates/signaltty-gui/src/terminal.rs`:
  persistent bar, `update_meta` toggle, click→answer, toast on error,
  `decision_render` pure fn + headless unit tests. No VTE rebuild.

## Phase 5: US3 - One channel per adapter (P3)

**Independent Test**: per-adapter fixture channel→bytes; no-channel →
read-only render.

 - [x] T013 [US3] `answerable` captured at ingest from adapter channel
  (router), asserted in integration test (codex `true`, claude `false`);
  GUI hint branch for `answerable:false`.
 - [x] T014 [US3] Docs + ADR: row in `docs/08-ipc.md` (method+events+code),
  entity in `docs/02-data-model.md`, `docs/adr/0008-decision-answer.md`;
  spec Assumptions append: live-Codex SC-001 follow-up.

## Phase 6: Polish

 - [x] T015 Gates: `cargo fmt --check`, `cargo clippy --workspace
  --all-targets` (zero new), `cargo test --workspace` green. GUI light+dark
  screenshot check of the bar BLOCKED (no display in harness; display tests
  hang under dbus-run-session without X/Wayland) — bar reuses pill/button
  classes + additive CSS only; mapping covered headless.
 - [x] T016 Commit(s): `area: what`, one concern per commit
  (server+core+proto / cli / gui / docs).

## Dependencies & Execution Order

T002→T003→T004→T005→T006 (foundational, in order) → T007→T008→T009→T010
(US1) → T011→T012 (US2) → T013→T014 (US3) → T015→T016.
