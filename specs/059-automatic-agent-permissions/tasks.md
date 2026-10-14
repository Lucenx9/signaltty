# Tasks: Automatic agent permissions

## Setup and foundation

- [x] T001 Record clarified scope and existing test seams in specs/059-automatic-agent-permissions/spec.md and plan.md.
- [x] T002 [US1] Add failing boolean persistence regression and implement default-off parsing in crates/signaltty-core/src/theme.rs.

## User story 1: Choose automatic execution once

- [x] T003 [US1] Add failing supported/unsupported launch coverage and implement pure native command generation in crates/signaltty-agent/src/execution.rs.
- [x] T004 [US1] Connect persisted native preference switch to installed workspace launch commands in crates/signaltty-gui/src/preferences.rs, app.rs and new_workspace.rs.
- [x] T005 [US1] Verify actual switch callbacks, settings save/load and light/dark native controls in crates/signaltty-gui/src/app_tests.rs.

## User story 2: Preserve orchestration choice

- [x] T006 [US2] Add failing resume regression then preserve generated automatic prefix and executable precedence in crates/signaltty-agent/src/resume.rs.
- [x] T007 [US2] Verify actual resumed process arguments through crates/signaltty-server/tests/automatic_execution.rs.
- [x] T008 [US2] Add failing default-worker tests then inherit same-provider prefix in crates/signaltty-server/src/tasks.rs; preserve explicit argv.

## Delivery

- [x] T009 Document decision and proof limits in docs/adr/0036-automatic-agent-permissions.md, docs/06-gui-toolkit.md, docs/07-agents.md and docs/08-ipc.md.
- [x] T010 Run scripts/verify.sh full and inspect generated light/dark native renders; record evidence in specs/059-automatic-agent-permissions/handoff.md.
- [ ] T011 Run independent Standards/Spec and final Gemini 3.8 Flash High Antigravity reviews, then create/link a PR.

## Dependencies and implementation strategy

T001 → T002 → T003 → T004 → T005. T003 → T006 → T007 → T008. T009 follows implementation, T010 and T011 follow all tests. Sequential changes reuse existing seams. Standards and Spec reviews can run independently. No parallel file writers.
