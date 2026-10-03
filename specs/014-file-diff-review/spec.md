# Feature specification: Per-file diff review

**Feature**: `014-file-diff-review`
**Created**: 2026-10-03
**Status**: Implemented and verified
**Input**: User approved adding a readable per-file diff to the existing Working Tree Changes GUI.

## User scenarios & testing

### User story 1: Read a changed file (P1)

As a developer supervising agents, I open Working Tree Changes and select a file to read what changed without leaving signaltty.

**Why this priority**: Counts alone cannot explain an agent's change.

**Independent test**: Modify a tracked text file, open Changes, activate its row, and read the added, removed, and context lines.

**Acceptance scenarios**:

1. Given staged and unstaged edits, selecting a file shows their combined changes against HEAD with three unchanged context lines where available.
2. Given several hunks, the reader shows their locations and old/new line numbers, with additions and removals distinguished by signs as well as color.
3. Given a deleted text file, the reader shows its removed content.
4. Given a clean workspace, the dialog reports no working tree changes.

### User story 2: Review files using the keyboard (P2)

As a developer, I navigate the file list and reader using the keyboard, refresh changes, and return to the list without losing my place.

**Why this priority**: Reviewing several files should not require repeated mouse navigation.

**Independent test**: Activate a selected row, scroll and select/copy text in its reader, return to the list, and refresh after another edit.

**Acceptance scenarios**:

1. Keyboard focus reaches file rows; Enter opens a diff and a keyboard-accessible control returns to the list.
2. Selecting another file or refreshing while a read is pending cannot display the earlier request as the new selection's diff.
3. Closing the dialog while a read is pending cannot reopen it or update a subsequent dialog.
4. At 360 pixels wide, the list and reader remain usable without clipped navigation controls.

### User story 3: Understand files that need special treatment (P2)

As a developer, I can identify new and binary files and understand when a preview is incomplete or unavailable.

**Independent test**: Review a new text file, binary file, oversized file, and a filename containing punctuation.

**Acceptance scenarios**:

1. A new text file shows its content as additions and remains labeled untracked.
2. Binary content produces an explicit binary notice instead of unreadable text.
3. A preview exceeding the size limit is visibly incomplete and cannot freeze the application.
4. A path containing spaces, tabs, newlines, markup, brackets, or Git pathspec syntax selects exactly that file and appears as literal text.
5. A repository without its first commit supports staged and untracked files; an unavailable or no-longer-changed file has a clear notice.

## Requirements

- **FR-001**: Working Tree Changes MUST let the user activate a file and read its unified diff against HEAD, including staged and unstaged edits.
- **FR-002**: The reader MUST show hunk locations, old/new line numbers, unchanged context, and explicit addition/removal signs.
- **FR-003**: Text MUST be selectable and copyable, displayed literally with monospace alignment, and readable in light and dark themes.
- **FR-004**: The dialog MUST provide keyboard navigation and preserve the selected file when returning to the list.
- **FR-005**: Reads MUST be asynchronous, bounded in size and duration, and ignore responses superseded by a newer selection, refresh, or dialog closure.
- **FR-006**: New text files MUST be readable as additions; binary files MUST have an explicit notice; truncation MUST be visible.
- **FR-007**: File reads MUST use repository-relative literal paths and MUST NOT follow a symlink to read content outside the repository.
- **FR-008**: Refresh MUST refresh the summary and invalidate the displayed patch; missing repositories/files and read errors MUST produce recoverable notices.
- **FR-009**: The feature MUST remain read-only: signaltty MUST NOT stage or edit files, mutate server workspace state, or operate on agents or terminal widgets. Tracked comparisons retain Git's trusted configured clean/process conversions, as the existing HEAD summary does.

### Key entities

- **Changed file**: Repository-relative path, tracked/untracked and binary status, existing addition/removal counts.
- **File diff**: Selected path, comparison base, text/binary status, completeness, hunks and numbered lines.
- **Review selection**: Selected file and current read generation, confined to the open dialog.

## Success criteria

- **SC-001**: A tracked-file fixture with two hunks displays every expected line and old/new line number correctly.
- **SC-002**: A keyboard-only user can open a file, read it, return, and select another file.
- **SC-003**: The dialog remains responsive while a read is pending; every read finishes or reports an error within ten seconds.
- **SC-004**: New, deleted, binary, unusual-name, and oversized-file fixtures each produce the expected preview or explicit notice.
- **SC-005**: Light, dark, and 360-pixel-wide captures show readable content and reachable navigation controls.

## Assumptions and scope

- This increment extends the current explicit-open/refresh Changes workflow. No background patch polling.
- The comparison remains HEAD, or an empty tree before the first commit. Per-turn snapshots remain future work under ADR-0011.
- Unified text is the default. Side-by-side views, syntax highlighting, staging, editing, blame, and rename inference are outside this increment.
- Existing file counts retain their current meaning; untracked previews do not silently change summary totals.
- Local Git and its configured canonical conversions are trusted workspace tools. They may have their own side effects; signaltty does not install or configure them. External diff and textconv presentation commands are disabled.
- Requirements are clarified from the user's accepted scope and existing product documents. No unresolved clarification markers remain.
