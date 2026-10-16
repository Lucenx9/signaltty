# Plan: Antigravity task orchestration

Use the existing native execution helpers and task resolver. Keep Antigravity a
generic screen-detected program. Add a task-specific inheritance entry point so
cross-provider Antigravity workers do not alter resume semantics. Match only the
recorded generated automatic prefix and bare worker commands.

Pair screen task transitions with Store events. Preserve blocker evidence across
initial submit and resume only screen-attributed interruptions after actual
activity. Do not synthesize native decisions or answer the TUI automatically.
Record the managed bare-command exception in ADR-0041 and current agent/IPC docs.

Constitution: clarified spec precedes implementation; existing public argument,
Store and isolated PTY/IPC seams support red/green tests. No new IPC method,
toolkit dependency, GUI behavior or state schema is required.

Validation: targeted argument and PTY regression tests, then the full shared
verification runner. Paid provider turns and changes to running workers are out
of scope. Verification evidence remains under ignored target/verification.
