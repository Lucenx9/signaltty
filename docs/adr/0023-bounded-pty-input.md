# ADR-0023: Bound native PTY input and cancel its actual I/O

Status: proposed, 2026-10-08 (ratified on PR merge).

## Context

Isolated blocking writers still retain descriptors and blocking-pool jobs after
pane close or child death. Returning a timeout around a blocking syscall would
hide that operation rather than release it. Portable-pty's writer destructor
also writes EOF bytes and can block.

## Decision

Set the native Linux master nonblocking before spawning its child. Own duplicated
File descriptors directly for input/output, bypassing the EOF-writing wrapper.
Descriptor clones share file flags, so the reader handles WouldBlock with readiness
polling and drains pending data on hangup. Input uses bounded polling, lifecycle
cancellation and a five-second budget from entry, including serialization waits.

Bind the original writer once. Dropping its registry handle cancels its active
and queued requests; server shutdown cancels all registered writers before
runtime teardown. The actual syscall is nonblocking, so cancellation ends the
job rather than abandoning a blocked write. Accepted bytes are a prefix of the
request and are reported on failure; they are never rolled back or retried.

## Consequences

Raw input failures expose written_bytes; worker failure reports null rather than
inventing a count. Shutdown can close the RPC connection before a canceled input
reply, but must release its jobs and exit. Scheduler saturation can delay responses
past the I/O budget; this is not a realtime response guarantee. The current
blocking pool remains finite. Automatic Enter retains its Store guard and can
wait up to the input budget; a separate try-only change will address that lock.

This is Linux-specific server I/O. Core remains OS-free. Native output, signal,
resize and replay are exercised through existing integration tests. No actor,
new task thread or client retry mechanism is introduced.
