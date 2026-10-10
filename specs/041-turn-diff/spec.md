# Specification: Latest-turn changes

**Branch**: `feat/turn-diff` | **Created**: 2026-10-10
**Status**: Implemented
**Input**: Follow-up to 040 ("Latest turn" scope was out of scope there). The Changes panel compares the checkout with HEAD only, so work an agent commits during its turn disappears from review. t3code scopes diffs to the agent's latest turn (docs/14 §6); ADR-0011 deferred this until the server records the worktree state at turn start.

## User scenarios and testing

### User story 1: See what the latest agent turn changed (P1)
An agent in the workspace receives a prompt, edits files, commits some of them and leaves others uncommitted.
Acceptance: With "Turn" selected, the panel lists every file the turn changed, committed or not, with `+N −N` against the checkout as it was when the turn started. Files that were already dirty before the turn and were not touched again are not listed. The totals line reads "Latest turn · N files · +A −R".

### User story 2: Keep the HEAD view (P1)
Acceptance: "All" stays the default and keeps today's behavior and wording ("Changes against HEAD"). The scope choice is one "All / Turn" toggle in the panel header (tooltips give the full names; short labels fit the 320px panel); switching scope re-reads the list and returns from the reader.

### User story 3: Read one file of the turn (P1)
Acceptance: In "Turn", activating a row opens the reader with that file's patch between turn start and now. The reader states the scope ("Changes in the latest turn"). A file with no remaining difference reads "This file no longer has changes in the latest turn."

### User story 4: Honest absence (P1)
Acceptance: Before any agent turn starts in the workspace (or after a server restart), "Turn" reads "No agent turn recorded in this workspace yet." instead of an empty list. A failed capture is reported, not shown as an empty turn.

### User story 5: The panel follows turns (P2)
Acceptance: While "Turn" is shown, a new turn baseline and the end of a turn (`done`, `idle`, `failed`, `blocked`) of a pane in the shown workspace refresh the list without user action.

### Edge cases
- A permission prompt inside a turn (`blocked` → `working`) does not start a new turn.
- Two agents in one checkout share it: the latest turn is the most recent turn start of any pane in the workspace, and lists every change since then.
- Capture never touches the user's index, refs, HEAD or files; it only writes Git objects.

## Requirements
- FR-001: On `agent.working` whose `prev` is not `blocked`, the server captures a tree of the workspace checkout (tracked and unignored untracked files) through a temporary index copy and records it as the workspace's turn baseline (runtime only, newest event wins).
- FR-002: Recording a baseline emits `workspace.turn_started {workspace_id, pane_id, started_at}`, also when its capture failed, so clients re-read and show the error instead of an older turn.
- FR-003: `workspace.diff` accepts `scope: "head" | "turn"` (default `head`). `turn` returns the same summary shape plus `turn: {pane_id, started_at}` (or `turn: null` with no files when no baseline exists), diffing the baseline tree against a fresh tree of the checkout. Every result carries `scope`.
- FR-004: `workspace.file_diff` accepts the same `scope`; `turn` returns the file's patch between the baseline tree and a fresh tree. Without a baseline it fails with `BAD_PARAMS`.
- FR-005: The CLI gains `signaltty workspace diff --turn`.
- FR-006: The GUI panel gains an "All / Turn" toggle and the refreshes of user story 5.

## Out of scope
Per-turn history (a turn picker), reverting a turn, baselines that survive a server restart, attributing changes to one of several agents sharing a checkout.

## Success criteria
Server integration tests cover capture on `working`, the blocked exception, committed and uncommitted turn changes, pre-turn dirt exclusion, `turn: null`, and the turn file read. Headless GUI tests cover the scope wording. `scripts/verify.sh fast` passes.
