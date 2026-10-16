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

## Process-refresh fixture correction

A combined full run exposed an intermittent initial spinner timeout before
prompt submission. A delayed-start probe reproduced it deterministically:
`pane.explain` reported `process: sleep`, no rules, and the visible Starting
spinner. The shell fixture's polling child replaced the scoped program during
/proc refresh. The executable `agy` fixture now uses Python and in-process
marker waits while emitting the same terminal text. The initial-submit case
releases its first screen after a periodic process scan and asserts the program
remains `agy`. This probe failed with the shell fixture and passes with Python;
it changes no production classification or timeout.
