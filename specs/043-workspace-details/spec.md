# Specification: Workspace details card

**Branch**: `gui/workspace-details` | **Created**: 2026-10-10
**Status**: Implemented and verified
**Input**: t3code's thread details card (host, editor, branch, PR with checks, Changes `+N −N`, lineage) as a card behind the header breadcrumb.

## User scenarios and testing

### User story 1: Know where a workspace is (P1)
Acceptance: Clicking the breadcrumb opens a card anchored below it. It shows the branch (or "Detached HEAD"; hidden outside git) and the `~` path, with the full path in its tooltip. Copy Path puts the full path on the clipboard and confirms with a check glyph for about a second. Open Folder hands the directory to the desktop's file handler. Running agents are listed when there are any.

### User story 2: See the task and its pull request (P1)
Acceptance: For a task workspace, a section shows "Task <label> · <state>". When the task has a PR, it shows `#N` and one status word: Merged, Closed, Checks failing, Checks running, Checks passing or Open, coloured by meaning. Activating that row opens the PR URL. Non-task workspaces hide the section.

### User story 3: Jump from totals to the diff (P1)
Acceptance: The Changes row shows "…" and then `+N −N` in the diff colours, "No changes", or "Unavailable". The totals come from `workspace.diff`, read only when the card opens; a slower read from an earlier open cannot overwrite a newer one. Activating the row closes the card and opens the docked Changes panel (`win.show-changes`).

### User story 4: Nothing to describe (P2)
Acceptance: Without a workspace the breadcrumb is inactive but keeps its look. Closing the last workspace closes an open card.

## Requirements
- FR-001: `details::details()` derives the card from the cached snapshot and `TaskIndex` (pure, unit-tested PR status mapping); `DetailsCard` builds the rows once and refills them on every show.
- FR-002: The breadcrumb becomes a `GtkMenuButton` with the card as its popover; no new IPC, state or dependencies.
- FR-003: New symbolic icons `signaltty-branch-symbolic` and `signaltty-pull-request-symbolic`, filled shapes only so symbolic recolouring works.

## Out of scope
Commit/push/PR actions, editor picker, lineage between orchestrator and workers, project scripts.

## Success criteria
GTK test `breadcrumb_opens_the_workspace_details_card` covers: inactive without a workspace, the card's branch, path, task, `#77 Checks passing`, totals from `workspace.diff`, Copy Path, and the hand-off to the Changes panel. Popover captured in Xvfb in light and dark. `scripts/verify.sh full` passes.
