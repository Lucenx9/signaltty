# Specification: Reconcile existing approval choices

**Feature Branch**: `gui/approval-reconcile-quality`
**Created**: 2026-10-08
**Status**: Ready for implementation
**Input**: Improve existing project and visual quality without new functions.

## User scenarios and testing

### User story 1: Show only current, answerable choices (P1)

A user sees the current options of an existing inline decision after state refresh, including loss of its answer channel after reconnect.

**Why this priority**: Old choices offer actions the server cannot accept.
**Independent test**: Update a native pane twice with the same decision ID and different answerability or options.

**Acceptance scenarios**:

1. Given an answerable decision, when the same ID becomes read-only, all choice buttons disappear and the existing terminal-answer hint appears.
2. Given that read-only decision, when the same ID becomes answerable, current choices return.
3. Given a changed option ID or label under the same decision ID, the displayed label and emitted answer use the current option.
4. Given unchanged options or a prompt-only update, the existing choice widgets and VTE remain mounted.

### Edge cases

- No pending decision hides the bar and clears its rendered cache.
- Empty options render no buttons.
- Changes to receive time alone retain the buttons.
- Decision ID changes replace callbacks as before.

## Requirements

- FR-001: Button reconciliation must track the decision ID, answerability and option IDs and labels.
- FR-002: Read-only decisions must have no answer buttons.
- FR-003: Updated buttons must emit the current IDs.
- FR-004: Unchanged choices and terminal widgets must retain their identity.

## Key entities

Reuse the existing Decision snapshot for the rendered cache. The wire model and server routes remain unchanged.

## Success criteria

- SC-001: A native regression fails before the fix when answerability changes under the same ID and passes afterward.
- SC-002: The same regression proves updated option IDs, read-only hint and stable widgets.
- SC-003: Full verification passes; native light/dark approval scenes are inspected.

## Assumptions

This enforces the existing decision contract from docs/02 and docs/06. No new lifecycle, API, control or answer-delivery behavior is introduced. Concurrent-click handling and resize retries are separate issues.
