# Specification: Isolate a stalled pane's input

Created: 2026-10-08. Status: clarified.

An orchestrator running several workers must be able to type into another pane
and stop a worker that is not consuming its PTY input. Server-owned sessions
and stable pane handles follow cmux/herdr's contract (docs/14 directives 4–5).
No GUI changes are needed.

## Requirements and acceptance

1. A raw-mode child that stops reading may block its own input request. It must
   not block input, resize, spawn or signal lookup for unrelated panes.
2. `pane.signal` must remain able to signal the stalled child. Preserve existing
   error codes, byte counts and input ordering. A signal acknowledgment from the
   child proves actual delivery while its input remains blocked.
3. Input requests for a single pane serialize through that pane's writer. The
   global handle registry must not be retained through blocking writer I/O.
4. A real-PTY regression fills an isolated child's input buffer, proves the
   request remains pending, then checks independent pane input and signal delivery.
   Cleanup must release the child even on the old failing implementation.
5. Raw input, prompt paste and terminal decision replies run blocking writer
   I/O outside Tokio executor threads. A single executor thread must still
   service state reads and independent pane requests during backpressure.

## Scope and clarification

This correction isolates existing blocking input ownership. It does not change
the `pane.input` wire contract, make all input nonblocking, or weaken the atomic
decision check around automatic Enter. The separate risks of a single-byte Enter
blocking under the Store guard and large writes outliving a dead child remain
for further diagnosis. The blocking pool has finite capacity. No unresolved
requirements remain within this scope.
