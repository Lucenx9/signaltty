# Feature Specification: Workspace visual refinement

**Feature Branch**: `gui/workspace-refinement`
**Created**: 2026-10-11
**Status**: Accepted
**Input**: Review visual adjustments, research T3 Code and similar projects, and submit improvements that make Signaltty feel professional.

## User Scenarios & Testing

### User Story 1 - Scan workspaces calmly (Priority: P1)
Read names, useful activity and branch context without repeated status words or competing selection decoration.
**Why this priority**: This navigation is visible throughout every coding session.
**Independent Test**: Populate working, done, attention and quiet shell rows and inspect their content and selection.
**Acceptance Scenarios**:
1. Given Working without timing, when the row renders, then only one Working label is visible; meaningful messages and elapsed durations remain.
2. Given selected and unselected rows, when navigating, then selection uses one quiet surface and lateral indicator; keyboard focus remains explicit.
3. Given branch or directory metadata, then a symbolic place indicator accompanies the text without squeezing attention or displacing the close control.

### User Story 2 - Use the same navigation at different sizes (Priority: P2)
Retain readable status and controls in light/dark, high contrast, narrow windows and enlarged text.
**Why this priority**: Visual polish must survive actual desktop settings.
**Independent Test**: Review native light/dark and 360px enlarged high-contrast scenes.
**Acceptance Scenarios**:
1. Given long titles and metadata, then text ellipsizes with full tooltips and no overlap.
2. Given high contrast, then selection, metadata and keyboard focus retain clear boundaries.
3. Given refresh and child disclosure, then row identities and terminal widgets stay stable.

### Edge Cases
- Bare state, timed state, explicit messages equal to state words, punctuation in meaningful messages.
- Empty metadata; directory fallback; child workspaces hiding root metadata.
- Narrow windows and larger system text; multiple urgent states.

## Requirements
### Functional Requirements
- **FR-001**: Suppress generated headlines that add no information to the status label, including a trailing ellipsis, while retaining explicit messages and durations.
- **FR-002**: Give metadata a quiet symbolic branch/directory indicator, hiding it when metadata is empty and with child metadata.
- **FR-003**: Keep sidebar selection, search and footer geometry coherent; distinguish selection from keyboard focus, and preserve high contrast.
- **FR-004**: Preserve existing status vocabulary, controls, navigation, tooltips, child disclosure, session ownership and terminal identity.

## Success Criteria
### Measurable Outcomes
- **SC-001**: Zero repeated generated bare-state headlines in the regression cases; all explicit message and timed cases retained.
- **SC-002**: No overlapping text or missing urgent controls in reviewed light/dark, narrow and enlarged high-contrast scenes.
- **SC-003**: Existing native navigation, refresh and terminal stability checks pass.

## Assumptions
- This is a bounded refinement of the current native desktop app, using its existing typography and theme.
- New navigation features, dependencies, fonts, IPC or state models are outside this pass.
- Visual observations from isolated displays do not certify real-desktop assistive technology or IME.

## Clarifications
No critical ambiguity remains: professional polish is translated into the above concrete acceptance cases; no functional redesign is required.
