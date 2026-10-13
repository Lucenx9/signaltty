# Feature Specification: Compact workspace hierarchy

**Feature Branch**: `gui/compact-workspace-hierarchy`
**Created**: 2026-10-13
**Status**: Ready for planning
**Input**: Improve sidebar density and header hierarchy toward T3 Code quality; use the requested design skills and deliver a PR.

## User Scenarios & Testing

### User Story 1 - Scan and select workspaces (Priority: P1)

As someone running several agents, I can scan workspace names and states without every row showing a full activity preview.

**Why this priority**: Sidebar navigation is repeated throughout the day.
**Independent Test**: Populate working, idle, unread, blocked and shell workspaces, select different rows and compare the retained content and height.

**Acceptance Scenarios**:
1. Given an unselected workspace without attention or tasks, its name and state share the primary line when space allows, with the state wrapping below when needed; its location occupies one metadata line.
2. Given a selected workspace, its useful activity preview is visible. Selecting another workspace restores the previous row's compact presentation immediately.
3. Given an unselected workspace requiring attention or with unread activity, its state and useful preview remain visible. Task chips remain visible regardless of selection.
4. Given a task hierarchy, children retain indentation and parent collapse remains reachable independently of selection.
5. Given a scoped project, filtering, selection restoration, duplicate-name disambiguation and explicit navigation continue to work.

### User Story 2 - Read the current workspace context (Priority: P1)

As someone working in a terminal, I can identify the current workspace and branch without a long directory consuming the header.

**Why this priority**: Workspace identity must remain clear beside creation and view controls.
**Independent Test**: Show git and non-git workspaces with long names and paths, then open workspace details.

**Acceptance Scenarios**:
1. Given a repository workspace, the header shows its name prominently and only its branch as secondary context, including agent workspaces.
2. Given a workspace outside a repository, the secondary context is its agent name when available or its directory name otherwise.
3. Given a long name, branch or path, full identity and location remain available through tooltips, accessible names and workspace details.
4. Given a narrow window or enlarged text, controls remain reachable and the existing compact toolbar behavior is preserved.

### Edge Cases

- Detached HEAD, blank branches, root directory, a home directory, literal markup characters and duplicate workspace names.
- Selection restored after a refresh, selected row filtered out, parent collapsed, children gaining attention, and task-chip updates between selection changes.
- Light, dark, high contrast, large text and right-to-left layout.

## Requirements

### Functional Requirements

- **FR-001**: Keep workspace name and textual state together when space allows, wrap the state below to protect the name at narrow widths or enlarged text, and retain at most one metadata line for ordinary unselected rows.
- **FR-002**: Reveal useful activity previews for selected or attention-bearing rows; always retain task chips and parent collapse controls.
- **FR-003**: Selection changes must update row detail visibility immediately, including programmatic selection and refresh restoration.
- **FR-004**: Keep full hidden metadata in the row's accessible name and a tooltip; do not rely on hover alone for task or decision information.
- **FR-005**: Prefer branch or detached-HEAD context in the header; otherwise show agents or a short directory name. Keep full path and name available in details and accessible/tooltip text. Narrow headers reserve space for the workspace name by hiding the secondary context.
- **FR-006**: Preserve terminal instances, project scope, navigation, rename/close behavior, keyboard focus and existing narrow toolbar controls.
- **FR-007**: Preserve the current theme palette and native input behavior; introduce no custom fonts or decorative motion.

### Key Entities

- Workspace presentation: existing workspace name, location, agents, activity, attention, task chips and selected state; no new persisted data.

## Success Criteria

### Measurable Outcomes

- **SC-001**: A fixed ten-workspace quiet fixture occupies less sidebar height than the same fixture before this change, with names and states visible.
- **SC-002**: All attention and task fixtures retain their state and useful content when unselected.
- **SC-003**: Switching selection changes preview visibility without replacing the workspace rows or terminal content.
- **SC-004**: Full workspace name and location remain retrievable with long-name and long-path fixtures in light/dark and narrow/large-text layouts.

## Assumptions

- The accepted immediate scope is sidebar and header refinement. Activity timeline, attention inbox and diff redesign are separate work.
- Existing native controls and desktop theme roles remain authoritative. The user's accepted restrained T3-inspired direction is preserved.
- Automated native display checks provide repeatable evidence; manual screen-reader speech, on-screen keyboard and IME checks are recorded separately rather than assumed to pass.
