# Feature specification: Task Board View in GUI

**Feature Branch**: `orch/board`
**Status**: Implemented
**Spec**: 019

## Goal

Provide a native GTK4/libadwaita Task Board dialog in the signaltty GUI that groups orchestrated worker tasks into kanban-style columns by what they need from the user.

## Column Derivation Rules

Tasks are purely mapped into four columns based on their state and disposition outcome:

1. **Working**:
   - `TaskState::Pending` or `TaskState::Working` with disposition outcome `None`.
   - Represents tasks actively executing or waiting for worker agent readiness.

2. **Needs you**:
   - `TaskState::InputRequired`, `TaskState::Failed`, or `TaskState::Rejected` with disposition outcome `None`.
   - Unfinished tasks that require human attention or intervention.

3. **In review**:
   - `TaskState::Completed` with disposition outcome `None`.
   - Finished tasks awaiting human diff review and finish decision (`merge` or `discard`).

4. **Done**:
   - Any task with disposition outcome `Merged` or `Discarded`.
   - Any task with `TaskState::Canceled`.
   - Archive column, sorted latest-first, displaying up to the latest 20 tasks while indicating the total count.

## UI Presentation

- **Dialog**: Presented via `adw::Dialog` with content width 960 and content height 600.
- **Empty State**: Displays an `adw::StatusPage` with "No tasks yet" when no tasks exist.
- **Columns**: 4 side-by-side columns, each with a header (`"<Column> · <N>"`) and a scrolled list of task cards.
- **Cards**:
  - Title: Bold task label (falls back to short id tail if empty).
  - Subtitle: `agent · branch · age` (formatted as dim text, e.g. `"codex · orch/board · 3m"`).
  - State pill: Native styled pill with `task-*` class and state word.
- **Interaction**: Clicking a card with an associated `pane_id` closes the board dialog and focuses that pane in the workspace.
- **Action & Shortcuts**:
  - Action name: `win.show-board`
  - Menu label: `"Show Task Board"` (in primary menu and searchable in command palette)
  - Shortcut: `<Control><Shift>b`

## Out of Scope (Deferred)

- **PR push / GitHub CI status / Pull Request review cycle**: Deferred to a subsequent specification because it requires an upstream decision and ADR on GitHub CLI (`gh`) integration, token handling, and remote provider credentials.
