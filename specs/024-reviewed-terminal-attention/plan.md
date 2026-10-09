# Implementation plan: Reviewed terminal attention

**Branch**: `t3/reviewed-terminal-attention` | **Date**: 2026-10-08 | **Spec**: [spec.md](spec.md)

## Summary

Take the pane snapshot before mutating and treat the two trailing terminal signals as no-ops on an already-terminal pane. `store::is_reviewed_terminal` gates the exit path (`set_exited` skips only the `unread` raise). The router suppresses the adapter's decision for a session-end hook that would set `done` on a pane already `done`/`failed`.

## Interpretation decisions

- Hook path keys on lifecycle only (`done`/`failed`), not on reviewed status. Reviewed `done`/`none` needs preserving (this bug) and `failed` must not become `done` even while still unreviewed; for `done`/`unread` the raise is already a no-op, and only the useful message is saved. This is a superset of "reviewed terminal only" that follows the explicit failure requirement.
- Exit path keys on reviewed (attention `none`) because unreviewed terminal panes already carry attention; lifecycle logic in `set_exited` already keeps `done`/`failed`.
- Not all new signals are deduplicated: the session-end/exit pair is the observed trailing signal. A `Stop` without a new prompt, notifications and BEL keep raising.
- A pending decision excludes the exit shortcut so the existing "exit drops decision and tells the user" behavior holds.

## Technical context

Rust 2021, min 1.92. `crates/signaltty-server/src/{store,router}.rs`. No new dependency, wire field, ADR or core change. Hook names compared case-insensitively (`SessionEnd`, `sessionEnd`) to cover Claude, Codex, Cursor.

## Constitution check

Spec precedes code (I). Red tests first (IV) at two seams: `Store` unit tests and `signaltty-testkit` hook IPC with a real PTY. State still changes only through Store transitions (V). Smallest change (VII); docs updated (VI). GUI untouched.

## Verification

Focused: `cargo test -p signaltty-server reviewed_terminal` plus the existing store/integration suites, `cargo fmt --check`, `cargo clippy -p signaltty-server --all-targets`. Full verify is the parent's gate after review.

## Red/green log

Recorded in tasks.md T003/T005.
