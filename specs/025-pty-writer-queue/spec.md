# Specification: Wait for a pane writer without consuming pool threads

Created: 2026-10-08. Status: clarified.

Many concurrent input requests to one stalled pane must not exhaust the blocking
pool and prevent another worker from receiving input. This extends the server
ownership and parallel-agent responsiveness requirements of docs/14.

## Acceptance

1. Async input binds its original PTY writer, then waits asynchronously for
   same-pane serialization before scheduling blocking I/O.
2. Only an actively writing async request occupies a blocking-pool thread;
   same-pane waiters do not. Different panes can progress on the remaining pool.
3. The serialization guard remains owned by the blocking operation if its
   awaiting future is canceled. Concurrent input bytes never interleave.
4. A deterministic regression limits the blocking pool to two threads, stalls
   one real raw PTY write, queues another request to that pane, and requires
   another pane's input to complete before releasing the first child.

## Scope

No new IPC methods or configuration. Delayed/retried Enter remains synchronous
and participates in the existing writer mutex. Cross-connection arrival order
is not a wire guarantee. Bounded kernel writes and automatic Enter are the next
specified changes. No unresolved requirements remain.
