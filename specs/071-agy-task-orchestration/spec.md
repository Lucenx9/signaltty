# Specification: Antigravity task orchestration

Created: 2026-10-16. Status: clarified.

An Antigravity 1.4.0 review worker stops at a native command approval.
Managed tasks must inherit an automatic parent launch choice and expose any
remaining screen-based interruption through the task API.

## Clarifications

The user selected inheritance when automatic execution is already active in the
parent. A supported automatic Codex/Claude/Antigravity parent may launch an agy
worker. A bare explicit agy argv is equivalent to its default launch; worker
options remain authoritative. No global default or live approval is changed.

## Requirements

- **FR-001**: `task.start` accepts `agent: agy`, `antigravity` or
  `antigravity-cli` without argv, keeping the generic detection kind.
- **FR-002**: A default or bare explicit Antigravity worker inherits
  `--dangerously-skip-permissions --mode accept-edits` when its recorded parent
  has Signaltty's automatic native prefix, including a different provider.
  Missing/guarded parents, custom commands and worker options preserve argv.
  Other providers retain existing inheritance behavior.
- **FR-003**: A screen-blocked worker task becomes `input_required`, including
  a blocker reached during initial submit, and `task.wait` with `settled` returns.
- **FR-004**: New screen activity resumes only an interruption attributed to
  that screen. Native decisions and terminal task results remain authoritative;
  repeated screen classification does not issue duplicate task events.
- **FR-005**: Real isolated PTY/IPC tests verify arguments and blocked/resumed
  states. The 1.4.0 permission-screen fixture must remain classified as blocked.

## Acceptance

An automatic parent starts a bare/default agy worker with automatic flags. A
guarded parent or explicitly configured worker preserves the requested mode.
A remaining permission screen parks the task immediately, and real resumed
activity clears the interruption. No paid provider turn is required for fixtures.
