# Specification checklist

Created 2026-10-04 for [task orchestration](../spec.md).

- [x] User value, scope, assumptions, and exclusions are explicit.
- [x] Acceptance scenarios cover each functional requirement.
- [x] Success criteria are measurable at public behavior boundaries.
- [x] Restart, retention, races, identity, cap, stall, conflict, and dirty-tree
  cases are covered.
- [x] Existing callers retain their default behavior (`pane.input`, `tail` /
  `screen` shapes, `wait` without baselines, `$SIGNALTTY_PANE`).
- [x] No unresolved clarification markers or constitution deviations remain.
- [x] The spec describes outcomes; implementation choices belong in the plan.
- [x] Every load-bearing choice cites its research source; disagreements are
  decided with reasons (spec §Deferred + ADR-0020).
- [x] The deterministic E2E scenario + failure paths are specified precisely
  enough to be the first tests written.
- [x] Async `task.start` returns `{task, pane}` at `pending`; the background
  ready + submit is server-owned, cancellable, and restart-safe (`pending` →
  failed `{stage: restart}`); cap refusal stays synchronous.
- [x] Turn end without report moves the task to `input_required`
  (`turn_ended_without_report`); follow-up submit returns it to `working`;
  `task.wait` defaults to `settled` (terminal or `input_required`).
- [x] Default worktree root is data-dir based with explicit `path` override;
  merge target is the recorded `target_branch` with a checkout-equality check
  at finish (detached-HEAD start requires explicit `target_ref`).
- [x] Test files are split per area (`e2e/submit/read/tasks/finish`) with
  shared helpers in testkit; the shipped skill carries the orchestrator loop;
  every new/changed method has an integration-test task and a docs/08 row
  task; all transitions go through Store transition methods (mutate + emit).
