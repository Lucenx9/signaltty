# Specification: Docked Changes panel

**Branch**: `gui/changes-panel` | **Created**: 2026-10-10
**Status**: Implemented and verified
**Input**: Follow-up to 039: replace the modal "Working Tree Changes" dialog with a t3code-style right-hand diff panel that sits beside the terminals.

## User scenarios and testing

### User story 1: Review changes without covering the terminals (P1)
A user presses Ctrl+Shift+D (or the header toggle, palette or main menu) while agents run.
Acceptance: A panel docks at the window's right edge, beside the terminals, which stay mounted and visible. The same key or the panel's close button hides it. Below 1100sp the panel overlays the content instead of squeezing it; below 760sp the main sidebar still collapses and the header toggle hides so the header fits 360px (spec 024).

### User story 2: Scan what changed by directory (P1)
Acceptance: Files sort by path. A dim directory label opens each run of files in the same directory, and top-level files have none. Each row shows the file name (full path in its tooltip) and, on the right, `+N −N` in the diff colours, or "Untracked"/"Binary" in place of counts. The totals line keeps "Changes against HEAD · N files · +A −R".

### User story 3: The panel follows the workspace (P1)
Acceptance: While the panel is open, switching workspace drops the old rows, the selection and any read still in flight, returns to the file list and loads the new workspace. Closing the last workspace clears it. A closed panel does no reads.

### User story 4: Read a file in place (P1)
Acceptance: Activating a row pushes the existing numbered, selectable reader inside the panel. Back (icon, Alt+Left, Escape) returns to the list with the row selected and focused. Refresh is an icon button with an accessible label on both pages. All 014 reader guarantees still hold: stale reads cannot overwrite a newer file, refresh invalidates the patch, and errors are explicit.

## Requirements
- FR-001: `changes::ChangesPanel` (persistent `AdwNavigationView`) replaces `changes::present`; `workspace_dialogs::changes` is removed.
- FR-002: A second `AdwOverlaySplitView` (sidebar at end, 320–440px, fraction 0.36) wraps the content page; libadwaita moves the window controls onto the panel header.
- FR-003: The `medium` (1100sp) breakpoint is added before `narrow` (760sp), because only the last matching breakpoint applies; `narrow` repeats the panel collapse.
- FR-004: The panel reuses the main sidebar colours through `--secondary-sidebar-*`, in every theme.
- FR-005: `show-changes` toggles the panel and gains Ctrl+Shift+D.

## Out of scope
"Latest turn" scope and refresh at turn end (needs server-side turn baselines; next spec). Syntax highlighting and split view in the reader.

## Success criteria
Ported GTK tests (`navigation_palette_fast_enter_and_git_dialogs_use_native_controls`, `file_diff_reader_uses_native_numbered_selectable_controls`) cover docking, grouping, counts, narrow overlay, workspace switching and the reader. `scripts/verify.sh full` passes; light/dark captures reviewed.
