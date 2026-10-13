# Accepted report retry verification

Branch `orch/idempotent-task-reports`, based on `f3f42f28e07eabcff3e9294938f9da49619be3c9`. Work runs in an isolated checkout and uses helper-owned servers, sockets, state, and repositories.

## Behavioral evidence

The initial core regression failed with `InvalidTaskTransition(Completed, Completed)`. After the core fix it passes for completed, failed, and rejected results, preserving task metadata across ten retries each. Field-specific mismatches and terminal tasks without results remain refused.

The IPC regression then reproduced twelve `task.result` events where one was expected: Store ignored the new no-op outcome. After Store and handler integration, all three focused `orchestrator_tasks` report tests pass. They verify reconnects, both selectors, optional defaults, unchanged timestamps, content refusals, concurrent acknowledgments with one event pair, canceled-task refusal, and retry after graceful snapshot restoration.

## Delivery gate and reviews

Full verification and final reviews are pending. The initial run `full-e1iet8yc` passed Clippy and warning policy, then was deliberately stopped before updating a review regression. It is not a passing delivery gate.

### Standards

Initial review: zero findings. Pure core matching, Store event pairing, typed IPC, and conservative scope conform to repository standards.

### Spec

Initial review: one mismatch in the new specification's removed-pane assumption. The existing task retains its pane association after closure. FR-008 requires preserving existing selectors, so the spec and contract now describe that retained association. A close-then-retry IPC case verifies both pane selection and explicit task ID. No routing change was introduced. Follow-up Spec review is pending.

### Final reviewer

Gemini 3.8 Flash High on Antigravity is the requested final reviewer. Its task, reviewed revision, findings, and responses will be recorded before PR completion.

## Limits

Snapshot restoration uses graceful shutdown. These tests do not establish durability of an acknowledgment before snapshot flush or delivery to a coordinator. No paid worker turn, automatic retry policy, or GUI behavior is added.
