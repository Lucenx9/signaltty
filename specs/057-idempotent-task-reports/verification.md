# Accepted report retry verification

Branch `orch/idempotent-task-reports`, based on `f3f42f28e07eabcff3e9294938f9da49619be3c9`. Work runs in an isolated checkout and uses helper-owned servers, sockets, state, and repositories.

## Behavioral evidence

The initial core regression failed with `InvalidTaskTransition(Completed, Completed)`. After the core fix it passes for completed, failed, and rejected results, preserving task metadata across ten retries each. Field-specific mismatches and terminal tasks without results remain refused.

The IPC regression then reproduced twelve `task.result` events where one was expected: Store ignored the new no-op outcome. After Store and handler integration, all three focused `orchestrator_tasks` report tests pass. They verify reconnects, both selectors, optional defaults, unchanged timestamps, content refusals, concurrent acknowledgments with one event pair, canceled-task refusal, and retry after graceful snapshot restoration.

## Delivery gate and reviews

`scripts/verify.sh full` passed on clean source revision `03a5e53016191e120e4a008d401a499521bccc4d`. The unique local evidence directory is `target/verification/full-il_hwkr2`; the [committed summary](evidence/verification.json) records the tested revision, clean state at launch, tool versions, passed steps, and limits. Workspace tests, server QA, all 42 isolated GTK tests, and the refresh benchmark passed. Subsequent changes record evidence only; Rust source and tests remain identical to the verified revision.

The initial run `full-e1iet8yc` passed Clippy and warning policy, then was deliberately stopped before updating a review regression. It is not a passing delivery gate. Representative light/dark task-board renders from the successful run were inspected; columns and state labels are readable. This change adds no GUI behavior.

### Standards

Initial and follow-up reviews: zero findings. The final reviewed source revision is `03a5e53`. Pure core matching, Store event pairing, typed IPC, and conservative scope conform to repository standards.

### Spec

Initial review: one mismatch in the new specification's removed-pane assumption. The existing task retains its pane association after closure. FR-008 requires preserving existing selectors, so the spec and contract now describe that retained association. A close-then-retry IPC case verifies both pane selection and explicit task ID. No routing change was introduced. Follow-up review on `03a5e53` reports zero findings and no unresolved objection.

### Final reviewer

Gemini 3.8 Flash High on Antigravity completed the final read-only review of `f3f42f28e07eabcff3e9294938f9da49619be3c9...03a5e53016191e120e4a008d401a499521bccc4d`. Verdict: PASS, zero findings and no blocking issue. It confirmed content matching, immutability, event/persistence suppression, serialized acceptance, restore compatibility, and retained selector semantics.

Provider/model: `antigravity` / `gemini-3.8-flash-high`. Delegation request: `signaltty-057-gemini-final-03a5e53-round1`. Review used inherited automatic permissions and requested no human confirmation. This is static review evidence; the owner performs the runtime gate. Subsequent changes record evidence only.

## Limits

Snapshot restoration uses graceful shutdown. These tests do not establish durability of an acknowledgment before snapshot flush or delivery to a coordinator. No paid worker turn, automatic retry policy, or GUI behavior is added.
