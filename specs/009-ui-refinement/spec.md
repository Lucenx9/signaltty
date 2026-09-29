# Feature Specification: Native workspace UI refinement

**Feature Branch**: `main`
**Created**: 2026-09-29
**Status**: Delivered to main; native verification and remote CI passed
**Input**: Refine the UI using animate, apple-design, emil-design-eng and relevant design skills, then push the changes.

## User Scenarios & Testing

### User Story 1 — Read and answer approvals in split panes (Priority: P1)

As a developer managing parallel agents, I can read the whole approval question
and reach every answer in a narrow pane, without leaving the terminal workspace.

**Independent Test**: Show a long question with three options in a 320px pane;
resize it and answer an option without losing terminal contents.

**Acceptance Scenarios**:

1. Given a pending approval, when the pane narrows, the question and choices wrap within the pane and remain readable.
2. Given a visible approval, when I traverse its choices by keyboard, the focused choice is visible and clearly marked.
3. Given an unanswered approval, when focus or pointer moves across the sidebar, its urgency remains visible.

### User Story 2 — Find and operate workspace controls (Priority: P1)

As a keyboard or pointer user, I can identify the selected workspace, focus and
activate pane controls, and distinguish project names, activity and location.

**Independent Test**: Navigate the shell and an inactive pane's buttons with the
keyboard, then inspect the same controls with the pointer in both themes.

**Acceptance Scenarios**:

1. Given an inactive pane, when its controls receive keyboard focus, they become visible immediately.
2. Given workspace rows with long names/messages/locations, their hierarchy stays legible and full text remains discoverable.
3. Given icon actions, assistive technology exposes their purpose through clear names.

### User Story 3 — Work with calm, responsive feedback (Priority: P2)

As a frequent user, navigation responds immediately and feedback respects my
desktop theme, accent, contrast and reduced-motion settings.

**Independent Test**: Navigate repeatedly, change motion settings, and inspect
pointer presses, state updates and focus in light/dark/high-contrast modes.

**Acceptance Scenarios**:

1. Keyboard workspace/tab/focus changes introduce no custom motion or waiting.
2. Pointer press feedback is subtle, interruptible, and never moves surrounding content.
3. Turning desktop animations off removes custom movement, including active button scaling.
4. Status changes do not rebuild terminal widgets or alter divider ratios.

### Edge Cases

- Long approval prompts, long option labels, and several choices in a small split.
- An urgent selected row while hovering or focusing its close action.
- A restored pane, an empty workspace, and a temporary server disconnect.
- Rapid pointer/keyboard focus changes and motion-setting changes at runtime.
- High contrast and enlarged system fonts.

## Requirements

### Functional Requirements

- **FR-001**: Approval questions and choices MUST adapt to pane width without clipping meaningful text.
- **FR-002**: Keyboard focus MUST be visible immediately on shell, row, pane and decision controls.
- **FR-003**: Urgency MUST remain visible while the workspace's close action is exposed.
- **FR-004**: Project name, activity, location and agent metadata MUST have a consistent visual hierarchy in both themes.
- **FR-005**: Icon controls MUST expose meaningful accessible names.
- **FR-006**: Frequent navigation MUST remain instant; custom feedback MUST be brief and interruptible.
- **FR-007**: Custom motion MUST honor desktop animation settings at launch and runtime.
- **FR-008**: Refinement MUST preserve existing attention, decision, terminal, divider and session behavior.

## Success Criteria

### Measurable Outcomes

- **SC-001**: A 320px pane displays a long question and three choices entirely within its bounds, and answering still succeeds.
- **SC-002**: Every tested icon or decision control is reachable and visibly focused; no inactive-pane control stays invisible when focused.
- **SC-003**: The same populated fixture is verified at desktop and narrow widths in light and dark themes, with no layout overflow.
- **SC-004**: Custom transitions take at most 200ms; keyboard navigation and reduced-motion interaction do not animate movement.
- **SC-005**: Existing reliability probes and project validation gates remain green.

## Assumptions

- The existing native Linux toolkit, system fonts and semantic desktop colors remain the design foundation.
- The primary job is the attention/approval loop for parallel coding agents.
- This refinement does not add a command palette, new agent integrations, backend contracts or decorative glass effects.
- Existing GTK display tests and isolated real-app probes are the verification seams authorized by the UI refinement task.
