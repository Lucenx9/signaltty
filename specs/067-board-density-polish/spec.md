# Feature specification: readable Task Board cards

**Feature Branch**: `gui/board-density-polish`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Refine the native app toward T3 Code visual quality in three independent PRs and bring them to merge. This slice addresses the Task Board.

## User scenarios & testing
### User story 1: scan task context (P1)
A user scans the title, context and worker state without the state consuming the context line.
**Independent test**: Open a board containing running, attention, completed and PR-linked tasks in light and dark.
**Acceptance scenarios**:
1. Given tasks with PR context, when opening the board, then context can occupy up to three full-width lines independently of the state.
2. Given a completed worker, when viewing a review column, then its state remains explicit but visually quiet; urgent states retain their emphasis.
### User story 2: keep the workspace visible (P2)
A user opens a board with a few tasks without a large empty sheet covering the workspace.
**Independent test**: Compare the board at desktop and narrow widths, then navigate a crowded board.
**Acceptance scenarios**:
1. Given a desktop window, when opening a sparse board, then its requested content height is at most 460px, reduced from 600px.
2. Given long titles, large text or many cards, then content remains navigable through existing scrolling and state navigation.
### Edge cases
Long unbroken branches, no tasks, stale PR observations, live task moves, keyboard focus, RTL, narrow windows and high contrast retain existing behavior.

## Requirements
- **FR-001**: Context must use the card width independently of state, wrapping up to three lines; title remains at most two lines.
- **FR-002**: Completed worker state must stay visible without a filled success badge; urgent states remain distinguishable by text and shape/color.
- **FR-003**: Board opening height must be reduced to at most 460px without resizing the dialog during live updates.
- **FR-004**: Existing navigation, live row identity, focus, scroll and task activation must remain intact across appearances and narrow/large-text layouts.
## Success criteria
- **SC-001**: PR-linked task context has no competing status allocation on the same line.
- **SC-002**: Native light/dark desktop captures show at least 140px less requested board height and explicit worker states.
- **SC-003**: Existing narrow and live-update scenarios pass without terminal reconstruction.
## Assumptions
Keep native interactions, system typography and semantic theme colors. Do not change task stages or PR eligibility. Centered shell identity stays unchanged.
## Clarifications
2026-10-15: accepted task authorizes autonomous polish, PR publication and merge. Three independent slices are Board, Worktrees toolbar adaptation and palette recognition. No remaining clarification is required.
