# ADR-0022: Isolate blocking PTY input by pane

Status: accepted, 2026-10-08 (ratified on PR merge).

## Context

`PtyManager::input` held the global handle registry across `write_all`. A raw
child that stopped reading prevented other pane input and even signal lookup
for that child. Blocking a Tokio executor thread also stalled store-only IPC;
moving only the writer lock did not make the real-PTY regression pass.

## Decision

Each handle owns an `Arc<Mutex<writer>>`. Lookup binds the writer and drops the
registry lock before serialization or I/O. Raw input, bracketed prompt paste and
terminal decision replies execute the blocking write in Tokio's blocking pool.
The binding happens before that handoff: queued bytes belong to the original
PTY and cannot be redirected to a replacement process with the same pane ID.
Dropping removed/replaced handles also occurs outside the registry lock.

Keep delayed/retried Enter synchronous under the Store read guard, preserving
the atomic decision check introduced in spec 023. Making that operation bounded
without restoring the permission race needs a separate design and regression.

## Consequences

Same-pane writes serialize; other pane input, resize and signal lookup remain
available while a child is not reading. The real-PTY test uses one executor
thread, exact received bytes and a child-side signal acknowledgment.

This is isolation, not a cancellation or timeout contract. Linux's large
blocking write can remain pending after the child dies (observed in a TERM probe
and debugger stack). A disconnected request does not roll back accepted bytes;
the blocking pool has finite capacity. Automatic Enter can still retain the
Store guard during backpressure. These limits are recorded for the next audit;
no new wire errors or partial-write semantics are introduced here.
