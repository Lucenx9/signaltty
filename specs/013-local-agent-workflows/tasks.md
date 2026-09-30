# Tasks: Local agent workflows

## Phase1: Setup
- [x] T001 Define clarified scope and quality checklist in specs/013-local-agent-workflows/spec.md and checklists/requirements.md.
- [x] T002 Ground and compare two designs; write plan/research/data-model/contracts/quickstart in specs/013-local-agent-workflows/.

## Phase2: Foundation
- [x] T003 Record native verdict lifetime and checkout ownership in docs/adr/0015-local-agent-workflows.md.

## Phase3: US1 Native permissions
- [x] T004 [P] [US1] Add red real-PTY reporter/API tests in crates/signaltty-server/tests/native_permissions.rs and provider-codec tests.
- [x] T005 [US1] Implement pure native prompt/verdict conversion and installer waiting PermissionRequest hooks in signaltty-agent and signaltty-integration.
- [x] T006 [US1] Implement ephemeral response broker, typed hook-event wait, consume/cancel and connection EOF/shutdown cleanup in signaltty-server.
- [x] T007 [US1] Connect CLI waiting reporter with native JSON-only stdout in signaltty-cli, and downgrade obsolete restored decisions.

## Phase4: US2 GUI navigation
- [x] T008 [P] [US2] Write real GTK navigation/search/zoom behavior tests in crates/signaltty-gui/src/.
- [x] T009 [US2] Implement action/workspace palette and rename through existing actions/API in signaltty-gui.
- [x] T010 [US2] Implement literal VTE search and local pane zoom with persistent widgets and unchanged server ratios in signaltty-gui.

## Phase5: US3 Worktrees and Git
- [x] T011 [P] [US3] Add red real-Git IPC tests in crates/signaltty-server/tests/worktrees.rs.
- [x] T012 [US3] Implement typed Git worktree lifecycle, association and race-safe removal in signaltty-server/src/worktrees.rs.
- [x] T013 [US3] Register methods/events/schema/params and CLI worktree operations in proto/server/cli.
- [x] T014 [US3] Implement native worktree and on-demand diff dialogs in signaltty-gui.

## Phase6: Verify and deliver
- [x] T015 Update docs/02-data-model.md, 06-gui-toolkit.md, 07-agents.md, 08-ipc.md, 09-persistence.md and README.md.
- [x] T016 Run standards/spec and adversarial review over feature changes; resolve accepted findings.
- [x] T017 Run build/fmt/clippy/workspace tests and relevant display tests; save inspected light/dark evidence in docs/qa/.
- [ ] T018 Commit verified feature on current branch with area:what concern separation.

## Dependencies and parallel ownership

T001->T002->T003 precede implementation. US1/US2/US3 red/green slices can run concurrently at separate files. Approvals own router/params/server/agent/integration/CLI main until finished; worktrees own new module, then shared registration is integrated sequentially. GUI owns GUI files. Review/gates follow all integration. Independent tests correspond to the three stories in spec.md.
