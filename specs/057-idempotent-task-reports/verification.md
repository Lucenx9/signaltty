# Accepted report retry verification

Branch `orch/idempotent-task-reports`, based on `f3f42f28e07eabcff3e9294938f9da49619be3c9`. Work runs in an isolated checkout and uses helper-owned servers, sockets, state, and repositories.

## Behavioral evidence

The initial core regression failed with `InvalidTaskTransition(Completed, Completed)`. After the core fix it passes for completed, failed, and rejected results, preserving task metadata across ten retries each. Field-specific mismatches and terminal tasks without results remain refused.

The IPC regression then reproduced twelve `task.result` events where one was expected: Store ignored the new no-op outcome. After Store and handler integration, all three focused `orchestrator_tasks` report tests pass. They verify reconnects, both selectors, optional defaults, unchanged timestamps, content refusals, concurrent acknowledgments with one event pair, canceled-task refusal, and retry after graceful snapshot restoration.

## Delivery gate and reviews

Full verification and independent reviews are pending. The final reviewer is Gemini 3.8 Flash High on Antigravity, as requested. Gate commands, actual revisions, outcomes, and review findings will be recorded before PR completion.

## Limits

Snapshot restoration uses graceful shutdown. These tests do not establish durability of an acknowledgment before snapshot flush or delivery to a coordinator. No paid worker turn, automatic retry policy, or GUI behavior is added.
