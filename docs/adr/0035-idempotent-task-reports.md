# ADR-0035: Idempotent task report acknowledgment

Status: proposed, 2026-10-14.

## Context

A worker can lose the response to an accepted `task.report`. The task is then terminal, so submitting the same report again fails even though the result was recorded. A new acceptance timestamp prevents comparing entire `TaskResult` values directly.

## Decision

Match status, exact summary, ordered artifacts, and decoded evidence against the task's accepted result. Exclude server-assigned `reported_at`. An identical report on the matching terminal state succeeds with the current task and changes nothing. Different content and terminal tasks without an accepted result retain `BAD_PARAMS`.

Core reports whether a mutation occurred. Store holds the write lock for matching and mutation and emits the existing `task.result` and `task.updated` pair only for a new report. The handler requests persistence only for a mutation. Existing saved results provide retry identity without a new key or schema.

## Consequences

Workers can retry identical content after reconnecting or after the accepted result is restored. Task lifecycle and review disposition remain immutable during retries. Pane lookup uses the task's retained association and can survive pane closure. Retain the task ID as the stable selector.

This acknowledgment describes current or restored server state. It does not add a synchronous snapshot flush, coordinator delivery, automatic retry loop, or a reopened execution. See [spec 057](../../specs/057-idempotent-task-reports/spec.md).
