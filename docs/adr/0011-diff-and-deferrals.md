# ADR-0011: Diff as Data, Turn Scoping Deferred

**Status**: Accepted (2026-09-29); decision 2 superseded by ADR-0029 · **Spec**: `specs/007-notif-actions-diff/`

## Context

Directive 6 (docs/14 §6) wants t3code run rendering: per-turn-scoped diffs
(`Latest turn ⌄`, `+N −N`, per-dir groups). signaltty had no diff
anywhere, and notifications offered only Focus.

## Decision

1. **`workspace.diff` serves worktree-vs-HEAD as data** (numstat files,
   per-dir rollups, totals; untracked/binary flagged). CLI, agents, and a
   future GUI badge consume the same numbers. Non-repo is `BAD_PARAMS`
   (loud). ADR-0015 adds literal NUL-separated filenames and disables rename
   detection; renamed files appear as deletion/addition.
2. **Per-turn scoping deferred.** A "turn diff" needs the worktree state
   at turn start; no such snapshot primitive exists, and faking it from
   the current diff would teach clients a lie. When lifecycle emits
   `working` we could record `git rev-parse HEAD` + status hash per pane
   — cheap, honest, and a clean follow-up.
3. **Collapse-and-count streams deferred.** Without transcript access
   (hooks carry summaries, PTY carries bytes) the server cannot type tool
   rows; parsing VT prose into narrative headers would be scraping by
   another name (forbidden by directive 3).
4. **Per-client seen-states deferred.** Single-user local machine: one
   global attention per pane is honest. Multi-viewer households are the
   trigger for per-client read states.
5. **GUI diff rendering delivered by ADR-0015.** The native Working Tree
   Changes dialog shows file counts and line totals, verified on a display.

## Consequences

- `signaltty workspace diff <handle>` works today; agents get `--json`.
- Notifications offer Focus + Mark read (both tested at the mapping seam).
