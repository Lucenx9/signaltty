# Specification: Bound PTY input and release it on lifecycle changes

Created: 2026-10-08. Status: clarified.

Closing or stopping a stalled worker must release input requests and their
resources. A successful server shutdown must not retain a worker blocked in a
kernel write. This fulfills server-owned sessions and predictable automation
per docs/14's cmux/herdr contract.

## Acceptance

1. Raw `pane.input` has a five-second I/O budget including its writer wait.
   A stalled write returns `TIMEOUT` with `details.written_bytes`; it never
   reports bytes that the kernel did not accept or rolls them back silently.
2. Pane close, child exit and server shutdown cancel the bound original writer.
   An in-flight request settles promptly with `PANE_EXITED` and its accepted
   byte count while its connection is live; shutdown may close the connection.
   Queued input cannot move to a replacement PTY.
3. No input syscall can block indefinitely. Dropping input ownership must not
   perform a blocking EOF write. The output pump handles the shared descriptor
   flags correctly, drains available output on exit, and remains readable.
4. Real isolated IPC tests reproduce pending input after close and child death;
   a timeout test verifies exact partial-byte evidence and later draining.
   Shutdown must actually exit the helper-owned process under stalled input.
5. Existing input, attach/replay, resize, signal, decision and submit behavior
   remains covered. Native permission responses and GUI rendering are unchanged.

## Scope and clarification

The five-second I/O bound is a server policy; scheduler saturation can delay
reply delivery beyond that budget. It is not a new IPC parameter. A caller
must inspect partial delivery before retrying. Each writer remains serialized
through the async gate from spec 025. Automatic Enter's Store-lock behavior is
preserved until its separate regression/fix; it must not reintroduce the
permission race. No unresolved requirements remain.
