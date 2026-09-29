# Tasks: Priority-Sorted Sidebar

**Input**: Design documents from `/specs/001-priority-sorted-sidebar/`

**Prerequisites**: plan.md, spec.md

**Tests**: Required by constitution IV (TDD red-green-refactor).

**Organization**: Single user story; rank first (core), then sort (GUI),
then wiring, then verification.

## Format: `[ID] [P?] [Story] Description`

## Phase 1: Rank in core (US1+US2 foundation)

- [x] T001 [US2] Add failing test `sidebar_rank_follows_directive_order`
  in `crates/signaltty-core/src/state.rs` covering all 7 lifecycles
- [x] T002 [US2] Implement `Lifecycle::sidebar_rank` in
  `crates/signaltty-core/src/state.rs`; `cargo test -p signaltty-core`

## Phase 2: Sort in GUI (US1)

- [x] T003 [P] [US1] Add failing tests for `sort_summaries` in
  `crates/signaltty-gui/src/sidebar.rs` (severity first, lifecycle
  order blocked/done/working/idle, recency, name tiebreak, stability)
- [x] T004 [US1] Implement `sort_summaries` in
  `crates/signaltty-gui/src/sidebar.rs`;
  `cargo test -p signaltty-gui` (headless)

## Phase 3: Wiring (US1)

- [x] T005 [US1] Sort `items` in `App::apply_refresh`
  (`crates/signaltty-gui/src/app.rs`) before `sidebar.update`;
  `cargo test -p signaltty-gui`

## Phase 4: Verification

- [x] T006 Run `cargo fmt --check`,
  `cargo clippy --workspace --all-targets`, `cargo test --workspace`
- [x] T007 Run ignored display test under `dbus-run-session` with
  `ADW_DEBUG_COLOR_SCHEME=prefer-light` and `=prefer-dark`:
  `dbus-run-session -- cargo test -p signaltty-gui -- --ignored --test-threads=1 event_batches`

## Dependencies & Execution Order

- T001 → T002 → (T003 → T004) → T005 → T006 → T007
- T003 is marked [P] only in the sense that the test file section is
  independent; it still needs T002's rank to compile the sort. Run
  sequentially; the whole feature is one sitting.

## Phase 5: Review follow-up

- [x] T008 Extend the ignored display test with rendered row-order +
  selection assertions (`assert_sidebar`: float, sink, rise cases);
  verified light + dark. A planned selection-restore in
  `Sidebar::update` was removed after red-proofs showed GTK keeps
  selection on moved rows — only the doc note + test remain.

## Notes

- Commit as `gui: priority-sort the workspace sidebar` after T007.
