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
and debugger stack). Independent review also reproduced `pane.close` returning
while its input request, blocking-pool thread and master fd remain alive.
`server.shutdown` replied but the server process was still alive after eight
seconds; runtime shutdown waiting for the blocking task is a hypothesis, not a
confirmed stack trace. The old implementation instead stalled the whole server.

A disconnected request does not roll back accepted bytes. Spec 025 adds a
per-writer async gate acquired before the blocking-pool handoff. Its owned guard
moves into the blocking operation, so cancellation of the awaiting future cannot
admit the next input early. Same-pane waiters do not consume pool threads; active
stalled writes still do. Ordering across concurrent connections remains a client
coordination responsibility. Automatic Enter can
retain the Store guard while waiting for another same-pane writer, as well as
while writing its byte. Portable-pty's writer destructor also writes EOF and can
block; dropping the registry entry outside its lock does not make destruction
bounded. These limits are recorded for the next audit; no new wire errors or
partial-write semantics are introduced here.
