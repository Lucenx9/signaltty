# Tasks: Per-file diff review

**Input**: spec.md, plan.md, research.md, data-model.md, contracts/ipc.md.
**Tests**: Constitution-mandated TDD at existing pure core, public IPC, actor and native GTK seams.

## Phase 1: Setup and foundation

- [x] T001 Clarify accepted scope and validate specs/014-file-diff-review/spec.md and checklists/requirements.md.
- [x] T002 Compare two architect candidates and synthesize specs/014-file-diff-review/plan.md, research.md, data-model.md, contracts/ipc.md and quickstart.md.

## Phase 2: Read a tracked file (US1, P1)

Independent proof: public IPC returns known two-hunk changes and the GUI displays their numbered context/addition/removal lines.

- [x] T003 [US1] Write and run a failing selected-file socket test in crates/signaltty-server/tests/file_diff.rs, then add typed params, canonical method/schema and async router entry.
- [x] T004 [US1] Write a failing pure parser test in crates/signaltty-core/src/diff.rs, implement shared typed hunks and expose the module in lib.rs.
- [x] T005 [US1] Implement bounded tracked-file HEAD/empty-tree reads in crates/signaltty-server/src/file_diff.rs and register in lib.rs; pass the tracked/deleted/staged+unstaged IPC fixtures.
- [x] T006 [P] [US1] Write failing reader-control assertions in crates/signaltty-gui/src/app_tests.rs, then implement native list activation and numbered TextView in changes.rs, workspace_dialogs.rs, main.rs and existing CSS.

## Phase 3: Keyboard and request lifecycle (US2, P2)

Independent proof: activate a row, scroll/select/copy, return to preserved selection and ignore old responses after refresh/Back/close.

- [x] T007 [US2] Implement and test generation invalidation, keyboard/focus Back and refresh behavior in crates/signaltty-gui/src/changes.rs and app_tests.rs.
- [x] T008 [US2] Write a failing dedicated-read concurrency test in crates/signaltty-gui/src/actor.rs, then isolate workspace.file_diff with a ten-second read deadline and accurate read error wording.
- [x] T009 [US2] Exercise actual GTK controls and capture light/dark/narrow reader evidence through crates/signaltty-gui/src/app_tests.rs.

## Phase 4: New, binary and bounded files (US3, P2)

Independent proof: public IPC and reader show correct states for new/binary/empty/large/unavailable files and exact unusual filenames.

- [x] T010 [US3] Add red/green IPC slices for new, binary, unborn, unusual names, subdirectory cwd, typed/traversal/nonmember params, symlink/special-file refusal and external-tool suppression in crates/signaltty-server/tests/file_diff.rs.
- [x] T011 [US3] Implement bounded no-follow file acquisition, total timeout/process cleanup and explicit truncation/unavailable states in crates/signaltty-server/src/file_diff.rs with pure parser edge tests in crates/signaltty-core/src/diff.rs.
- [x] T012 [US3] Exercise binary/untracked/empty/error/incomplete reader states and literal rendering in crates/signaltty-gui/src/changes.rs and app_tests.rs.

## Phase 5: Integrate, review and verify

- [x] T013 Record the durable decision in docs/adr/0016-file-diff-review.md and update docs/02-data-model.md, docs/06-gui-toolkit.md, docs/08-ipc.md and the local verification feature map.
- [x] T014 Review since edf10ce with independent standards/spec and adversarial checks; resolve accepted findings and record evidence in specs/014-file-diff-review/verification.md.
- [x] T015 Run cargo build --workspace, cargo fmt --check, cargo clippy --workspace --all-targets, cargo test --workspace and relevant ignored GTK tests; inspect actual screenshots and finish commit/push.

## Dependencies and parallel work

T001 → T002 gates all code. T003 → T004 → T005 completes the first server slice. T006 can proceed in separate GUI files against the fixed contract while server owners implement T003–T005. T007/T008 follow the reader slice; T010/T011 follow the tracked slice and T012 follows the reader. T009/T013 follow completed behavior; T014 → T015 integrates the feature. Root alone owns planning/docs/task status and Git mutations; server and GUI owners never edit each other's files.

## Delivery strategy

Prove tracked text first, then lifecycle/keyboard, then special content and bounds. Each new behavior gets a failing assertion before its implementation. Finish the full accepted increment, including visual proof and workspace gates. No external issue publication is part of the authorized work.
