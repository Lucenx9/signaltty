# Tasks: Reliable workspace interactions

## Setup and foundation

- [x] T001 Ground docs06/14/15/16/17, constitution and applicable skills; write spec/plan/contracts in specs/052-reliable-workspace-interactions/.
- [x] T002 Verify prerequisites and record base/branch and test-first evidence in specs/052-reliable-workspace-interactions/handoff.md.

## US1: Protect running terminals

- [x] T003 [US1] Add failing cancellation/target/failure/exited regressions in crates/signaltty-gui/src/app_tests.rs.
- [x] T004 [US1] Implement running-session confirmations and reply-driven tab close in crates/signaltty-gui/src/app.rs.
- [x] T005 [US1] Verify native light/dark confirmation and identity preservation in crates/signaltty-gui/src/app_tests.rs.

## US2: Correct and retry workspace creation

- [x] T006 [US2] Add invalid input, pending/failure/retry regressions in crates/signaltty-gui/src/new_workspace.rs and app_tests.rs.
- [x] T007 [US2] Implement native persistent action dialog with field validation and pending/retry feedback in crates/signaltty-gui/src/new_workspace.rs.
- [x] T008 [US2] Preserve partial workspace id and refresh/retry command startup in crates/signaltty-gui/src/app.rs.
- [x] T009 [US2] Verify create failure, partial command failure and single-workspace successful retry in crates/signaltty-gui/src/app_tests.rs.

## US3: Find compact tools

- [x] T010 [US3] Extend compact tool behavioral regression in crates/signaltty-gui/src/app_tests.rs before implementation.
- [x] T011 [US3] Add labeled compact sidebar tools bound to existing actions/state in crates/signaltty-gui/src/app.rs.
- [x] T012 [US3] Verify 360px, large-text/high-contrast and wide visibility in crates/signaltty-gui/src/app_tests.rs.

## Delivery

- [x] T013 Update docs/06-gui-toolkit.md and .agents/skills/verify-signaltty/features/; adapt scripts/qa-gui-reliability.py to Cancel/confirm the native close dialog.
- [x] T014 Run full verification, inspect captures and retain selected evidence under docs/qa/reliable-workspace-interactions-2026-10-12/.
- [x] T015 Run independent standards/spec reviews and fix findings; record in specs/052-reliable-workspace-interactions/verification.md.
- [ ] T016 Push branch, open/link PR and update specs/052-reliable-workspace-interactions/handoff.md.

## Dependencies and implementation strategy

Setup precedes tests; each story's red regression precedes its production edits. US1 is the first independent deliverable. US2 and US3 are behaviorally independent but share app.rs/app_tests.rs, so one writer executes sequentially. Independent review axes may run in parallel once the diff is stable. Tests on one GTK process cannot run in parallel threads; use separate Xvfb/D-Bus processes. No extension hooks are configured.
