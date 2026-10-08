# Specification: Reviewed terminal attention stays reviewed

**Feature Branch**: `t3/reviewed-terminal-attention`
**Created**: 2026-10-08
**Status**: Ready for implementation
**Input**: Close the attention loop (directive 1, `docs/14`): a finished turn the user already read must not light up again when the agent process merely shuts down.

## Problem

`docs/03` defines "turn finished, user reviewed" as `done` / `none`. Two trailing signals re-raise `unread` on that settled pane:

1. A `SessionEnd` (Claude, Codex) or `sessionEnd` (Cursor) hook. Each adapter maps it to `done` + `unread` + message "session ended"; the router applies it unconditionally.
2. `Store::set_exited` raises `unread` for every pane whose PTY child exits.

Observed sequence: `Stop` (done/unread) → user focuses the pane (`mark_seen`, done/none) → the user quits the agent → `SessionEnd` → done/unread, last message replaced by "session ended" → PTY exit → unread again. A pane the user has finished with returns to the jump-to-next-unread queue. A failed pane also loses its outcome: `SessionEnd` overwrites `failed` with `done`.

## User scenarios and testing

### User story 1: Quitting a reviewed agent is silent (P1)

**Independent test**: real hook IPC on a live pane: `Stop`, `pane.mark_seen`, `SessionEnd`; then PTY exit.

**Acceptance scenarios**:

1. Given `done`/`none` after review, when a session-end hook arrives, lifecycle stays `done`, attention stays `none`, and `last_message` keeps the Stop summary. Applies to Claude, Codex and Cursor.
2. Given the same pane, when its PTY child then exits, attention stays `none` and `live` becomes `exited` with the code.
3. A session id carried by the session-end payload still updates identity and resume argv.

### User story 2: A terminal outcome is not rewritten at session end (P1)

1. Given `failed` (with or without `error` attention), a session-end hook leaves lifecycle `failed`, the attention it had, and the failure message.
2. Given `done`/`unread` (not yet reviewed), a session-end hook keeps `done`/`unread` and the summary.

### User story 3: Everything that should alert still alerts (P1)

1. First completion: `working` → session end → `done`/`unread` ("session ended"). Working exit raises `unread`.
2. `idle` (session opened, no turn) → session end → `done`/`unread`.
3. Fresh turn after review: `UserPromptSubmit` (working/none) then `Stop` → `done`/`unread`.
4. Blocked panes keep their gates: `permission_required`/`input_required` and a pending decision retain existing session-end behavior. Process exit clears the decision and demotes required attention to unread, as before (`docs/08`).
5. BEL, OSC notifications and push notifications are unchanged.

### Edge cases

- Exit with a pending decision still drops it and raises `unread`.
- A repeated `mark_seen` after exit remains idempotent.
- Unknown or non-session-end hooks are unaffected.

## Requirements

- FR-001: A session-end hook that would set `done` on a pane already `done` or `failed` is a no-op for lifecycle, attention and last message. Session identity and resume argv are applied first, as today.
- FR-002: `Store::set_exited` must not raise `unread` when, before the exit, the pane was `done`/`failed` with attention `none` and no pending decision. All other exit effects (live state, `pane.exited`, task failure, decision/attention demotion) are unchanged.
- FR-003: Behavior for non-terminal panes (`unknown`, `idle`, `working`, `blocked`) is unchanged.
- FR-004: No wire, adapter, GUI or notification change.

## Key entities

Existing `Pane.lifecycle`, `Pane.attention`, `Pane.pending_decision`, `Pane.last_message`. The predicate "reviewed terminal" (lifecycle `done`/`failed`, attention `none`, no decision) is derived, not stored (ADR-0005: lifecycle and attention stay independent).

## Success criteria

- SC-001: Store and real-IPC regressions fail before the change and pass after, with the actual red output recorded in `plan.md`.
- SC-002: The existing store/router/integration suites pass unchanged.
- SC-003: `docs/03` and `docs/07` state the contract.

## Assumptions

Only the trailing session-end hook and the PTY exit are deduplicated. A genuinely new signal on a reviewed pane (new turn, notification, permission request, BEL, error) re-raises attention as before. `Stop` after review without an intervening prompt is not covered by this spec.
