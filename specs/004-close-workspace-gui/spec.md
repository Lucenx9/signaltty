# Close workspaces from the GUI

**Branch**: `codex/close-workspace`
**Created**: 2026-09-29
**Status**: Ready

## User story

A user can close a workspace from its sidebar row, or close the active
workspace from the main menu. A confirmation names the workspace and warns
that its running terminals and agents will stop. Cancel leaves it intact;
confirm removes it from the GUI after the server accepts the request.

## Acceptance scenarios

1. Given two workspaces, closing either from its row targets that row's id,
   even if another workspace is selected.
2. Given an active workspace, the menu's Close Workspace action targets it.
3. Cancel or Escape sends no `workspace.close` request.
4. Confirm sends exactly one `workspace.close` request. On success the sidebar
   and active content refresh; on failure the workspace stays visible and a
   toast reports the error.
5. Repeating a close action while its confirmation is open does not open a
   second confirmation.

## Scope and assumptions

- Reuse the existing `workspace.close` IPC method and its process handling.
- Closing a workspace does not delete its project directory.
- No new server method, persistence format, or model field is needed.
