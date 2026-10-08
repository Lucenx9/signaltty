# Specification: Task Board readability and narrow-window correction

**Feature Branch**: `gui/quality-polish-20261008`
**Created**: 2026-10-08
**Status**: Ready for implementation
**Input**: Improve project and visual quality without new functions.

## User scenarios and testing

### User story 1: Read every board column in a narrow window (P1)

A user opens the existing Task Board in a 360px window and reaches every column and its cards.

**Why this priority**: Off-screen tasks prevent the existing review workflow.
**Independent test**: Render the native board at 360px, scroll to Done, and activate its card.

**Acceptance scenarios**:

1. Given a populated board and a 360px window, opening the board keeps the window and dialog within that width.
2. Given columns beyond the visible area, native horizontal scrolling and keyboard focus reveal their cards.
3. Given a card with a pane, activating it preserves the existing close and focus callback.

### User story 2: Read secondary text in each appearance (P2)

A user reads agent, branch and age without overlapping opacity reductions.

**Why this priority**: Branch context supports existing task decisions.
**Independent test**: Inspect light, dark and high-contrast native renders, including enlarged text.

**Acceptance scenarios**:

1. Given normal appearance, metadata has one secondary text treatment.
2. Given high contrast, column headers and metadata have full opacity and inherit readable text color.

### Edge cases

- Empty boards keep their existing status page.
- Long labels and branch names remain ellipsized with complete tooltips.
- Large text retains scroll access to all five columns.
- Cards without a pane remain nonactivatable.

## Requirements

- FR-001: The board must fit a 360px parent window without forcing it wider.
- FR-002: Every existing column must remain reachable by native scrolling and keyboard focus.
- FR-003: Column classification, counts, card activation, and task requests must remain unchanged.
- FR-004: Secondary text must not combine dim styling with an additional opacity reduction.
- FR-005: High contrast must remove custom muting from board headings and metadata.

## Key entities

Existing Task, TaskCardView and BoardColumnView remain unchanged.

## Success criteria

- SC-001: Native 360px light and dark scenes show readable cards within the parent bounds.
- SC-002: Scrolling reaches the last column and its card activation delivers the original pane identifier.
- SC-003: Existing wide board and empty-state tests pass, alongside the full verification gate.

## Assumptions

This corrects existing presentation. There are no new commands, task states, protocol fields or dependencies. Native GTK scrolling provides the overflow behavior. No constitution deviation is required.
