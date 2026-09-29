# Feature Specification: Notification Actions + Worktree Diff

**Feature Branch**: `007-notif-actions-diff`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "t3code visual/quality bar (docs/14 §6–7) and
the cmux inline-action loop (directive 1): OS notifications carry only
Focus, and there is no worktree diff anywhere (no +N −N, no per-dir
groups) — the sidebar and panes render `tail -f` plus a headline."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Act from the notification (Priority: P1)

A notification arrives while the user is elsewhere: Focus jumps to the
pane, Mark-read clears the attention without leaving the current context.
Two actions, no new windows.

**Independent Test**: pure action→event mapping unit test; action names
asserted in code (server roundtrip covered by existing mark_seen tests).

**Acceptance Scenarios**:

1. **Given** an attention notification, **When** the user picks Mark-read,
   **Then** `pane.mark_seen` runs and the badge clears on next refresh.
2. **Given** `SIGNALTTY_NOTIFY=0`, **When** attention fires, **Then** no
   popup at all (existing kill-switch unchanged).

### User Story 2 - Worktree diff as data (Priority: P2)

`signaltty workspace diff my-api` prints per-file `+N −N`, per-dir
groups, and totals — the numbers the t3code "Latest turn ⌄" language
needs, from the server so agents get them too (`--json`).

**Independent Test**: fixture repo (committed file + modification +
untracked file) → `workspace.diff` returns numstat rows, dir rollup,
totals; outside a repo → `BAD_PARAMS`.

**Acceptance Scenarios**:

1. **Given** a repo with `a.txt` (+3/−1) and untracked `new.txt`,
   **When** diffed, **Then** files list both (untracked as `+/−` unknown
   → `added: null`? No — untracked: `untracked: true`, no counts),
   dirs roll up, totals count tracked only.
2. **Given** a non-repo cwd, **When** diffed, **Then** `BAD_PARAMS`
   (loud, not an empty lie).

## Requirements *(mandatory)*

- **FR-001**: Notification offers exactly `Focus` + `Mark read`
  (notify-rust actions `focus`/`mark-read`); Mark-read sends a new
  `UiEvent::MarkSeen(pane)` handled by `App` via `pane.mark_seen` +
  refresh (reuse, no new IPC).
- **FR-002**: `workspace.diff {workspace_id}` → `{workspace_id, branch?,
  files: [{path, added?, removed?, untracked?, binary?}],
  dirs: [{dir, added, removed}], added, removed}` from
  `git diff --numstat HEAD` + `--porcelain` untracked names. Binary rows
  (`- - path`) carry `binary: true`, zero counts.
- **FR-003**: `workspace_id` accepts handle-or-id (resolver reuse).
- **FR-004**: Per-turn scoping, GUI diff badges, collapse-and-count
  streams, and per-client seen-states are explicitly deferred (reasons in
  ADR-0011): turn scoping needs worktree snapshots at turn start (no such
  primitive), GUI needs a display to verify (blocked in harness),
  single-user local makes global attention honest.

## Success Criteria *(mandatory)*

- **SC-001**: Both notification actions work from a real desktop
  (manual; code path unit-tested).
- **SC-002**: Diff numbers match `git diff --numstat` on the fixture.
