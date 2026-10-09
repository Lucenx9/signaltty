# Plan: Isolate a stalled pane's input

Start with a real two-pane IPC regression: one child enters raw mode and waits
on an external file before reading; a large input request saturates its buffer.
The other pane must accept input and the stalled child must acknowledge a
signal before the release file is written. Record the red trace before editing
production code.

The red loop also stalls a store-only status request; isolating only the writer
lock does not turn it green. Move raw input, bulk paste and terminal replies to
Tokio's blocking pool, binding the writer before the handoff so a queued request
cannot resolve to a respawned pane's new PTY. Pin the test server to one executor
thread to verify this dependency explicitly.

Use per-pane writer ownership so the registry lock covers lookup only; serialize
same-pane writes independently. Preserve portable-pty lifecycle ownership and
signal/resize paths, with no async/OS logic moved into core. Check exit/destroy
and respawn interactions, and record the locking decision in an ADR.

Run the focused regression, submit regressions and shared full verification.
Document local native-library failures separately from CI proof. Independent
Grok 4.7 xAI and Sonnet 5.5 reviews cover the final diff.
