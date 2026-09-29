# Plan: close workspaces from the GUI

**Spec**: [spec.md](spec.md)
**Branch**: `codex/close-workspace`

## Design

Add a subtle close button to each persistent sidebar row and a Close Workspace
entry to the central window-action registry. Both pass a captured workspace id
to one App method. Show a single `AdwAlertDialog` with a destructive response;
only its confirmation calls `workspace.close`. Refresh the authoritative
workspace list after success, letting existing reconciliation select a
remaining workspace or show the empty state.

## Verification

Use the GUI's display-test seam with a fake IPC actor to check cancel,
targeting, confirmation, and refresh. Run the workspace fmt, clippy, and test
gates, then inspect the sidebar controls in light and dark mode if a display is
available.

## Constitution check

The spec precedes code; the GUI remains an IPC client; the existing server
transition and event are reused. No new architectural decision is introduced.
