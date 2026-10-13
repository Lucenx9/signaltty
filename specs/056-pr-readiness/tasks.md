# Tasks: Verified PR readiness

## Setup

- [x] T001 Create and clarify specs/056-pr-readiness/spec.md and checklists/requirements.md.
- [x] T002 Complete specs/056-pr-readiness/plan.md, research.md, data-model.md, contracts/ipc.md, and quickstart.md.

## User story 1: Trust the merge queue

- [x] T003 [US1] Reproduce unsafe readiness with a regression in crates/signaltty-gui/src/board.rs before implementing the core projection.
- [x] T004 [US1] Add serde-defaulted TaskPr facts in crates/signaltty-core/src/model.rs and pure TaskStage/readiness rules with domain tests in crates/signaltty-core/src/workflow.rs.
- [x] T005 [US1] Replace board derivation in crates/signaltty-gui/src/board.rs and migrate existing fixtures to explicit recent merge facts.

## User story 2: Recover from failed observations

- [x] T006 [US2] Add typed gh observation, failed-refresh invalidation, batch errors, and older-attempt rejection in crates/signaltty-server/src/tasks.rs, with regressions in crates/signaltty-server/tests/orchestrator_pr.rs.
- [x] T007 [US2] Add descriptive PR notes in crates/signaltty-gui/src/board.rs and update the open board on the existing 30-second tick in crates/signaltty-gui/src/app.rs.

## User story 3: Share one coordination status

- [x] T008 [US3] Expose derived stage in task get/list/refresh views in crates/signaltty-server/src/tasks.rs and print stage and batch failures in crates/signaltty-cli/src/main.rs.

## Verification and delivery

- [x] T009 Record ADR-0034 in docs/adr/0034-verified-pr-readiness.md and update docs/02-data-model.md, docs/08-ipc.md, and relevant verification guidance.
- [x] T010 Run scripts/verify.sh full, inspect native board renders, and record results in specs/056-pr-readiness/verification.md.
- [x] T011 Review the diff, commit the concern, push orch/pr-readiness, create and link the PR.

## Dependencies and execution

T001 and T002 precede implementation. US1 supplies the domain rule for US2 and US3. Each test precedes its behavior change and uses existing domain, IPC, CLI, or GTK seams. Shared files are edited sequentially. Documentation and unrelated-file verification can run in parallel once contracts are stable. This feature ships as one PR without automatic feedback or runtime architecture changes.
