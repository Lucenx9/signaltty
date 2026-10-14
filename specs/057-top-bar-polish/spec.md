# Feature Specification: Top bar polish

**Feature Branch**: `gui/top-bar-polish`
**Created**: 2026-10-13
**Status**: Implemented and verified
**Input**: Continue the authorized GUI polish, focusing on the top bar and researching web best practices; deliver the next PR.

## User Scenarios & Testing

### User Story 1 - Identify and navigate the workspace (Priority: P1)
Users can read the current workspace in the top bar while navigation/creation and view controls form distinct groups.
**Independent Test**: Open a long-name workspace, switch between desktop and narrow sizes and compare readable title allocation.
**Acceptance Scenarios**:
1. Given a wide window, navigation/creation is at the leading edge, workspace identity has a distinct region and attention/views/menu remain trailing.
2. Given a 360px window with enlarged text, the title has more useful width than the current ellipsis-only baseline while New Tab, Sidebar, attention and Main Menu remain reachable.
3. Given a repository workspace, short branch context remains secondary on desktop, with complete metadata accessible through Details and tooltips.

### User Story 2 - Use the bar across input and window modes (Priority: P1)
Users can operate toolbar actions by keyboard and retain native window behavior.
**Independent Test**: Keyboard focus, RTL order, repeated resizing, active attention and Details interaction with a retained terminal.
**Acceptance Scenarios**:
1. Given narrow/desktop transitions, controls return to their appropriate presentation, with one visible Board/Changes entry point and preserved toggle state.
2. Given RTL direction, functional groups mirror without overlapping; keyboard focus reaches the intended controls.
3. Given a desktop bar, there is a non-interactive blank region available for native window dragging and native window controls remain present.
4. Given selection or resize, the active terminal instance and content remain unchanged.

### Edge Cases
Empty application title, very long generated workspace/branch names, full paths with literal markup, attention counter shown/hidden, enlarged font, high contrast, RTL and repeated breakpoint transitions.

## Requirements
- **FR-001**: Group navigation and creation at the leading edge, allow workspace identity to use the remaining title region, and keep attention/views/menu at the trailing edge.
- **FR-002**: Preserve readable identity under narrow/enlarged-text conditions; decorative context or marks may hide before actionable controls.
- **FR-003**: Retain complete metadata in the accessible Details control and tooltip.
- **FR-004**: Preserve action meaning, accelerators, focus, one visible Board/Changes entry point and Changes toggle synchronization across breakpoints.
- **FR-005**: Retain native window controls and a non-interactive drag region without replacing native window chrome.
- **FR-006**: Preserve terminal and row identity through resize/navigation; no protocol, server or persisted-state changes.
- **FR-007**: Use the established restrained surfaces, semantic theme roles and system font; group controls through spacing rather than additional boxed panels.

## Success Criteria
- **SC-001**: The long-name 360px enlarged-text fixture gives the title more horizontal space than its baseline, with no required control outside the window.
- **SC-002**: Desktop/light/dark/RTL/large-text/native-high-contrast fixtures retain distinct identity, action groups and visible keyboard focus without overlap.
- **SC-003**: Existing actions and compact tools remain usable through repeated size changes, and the active terminal instance is retained.
- **SC-004**: The desktop fixture retains a blank drag region of at least 24px and a compact fixture of at least 12px.

## Assumptions
The active task continues the previously authorized PR workflow. #104 is merged; baseline is `9d1913a`. Final integration base is `f3f42f2`, including #105. Scope is top-bar presentation and native adaptive layout, not new timeline/inbox/navigation features. Actual desktop dragging, screen-reader speech, OSK and IME are explicitly reported if not exercised; native geometry alone does not certify compositor behavior.

## Full-width grouping correction, 2026-10-14
User screenshot shows the workspace mark, centered name and far-edge branch dispersed across a fullscreen header. Extend FR-001/SC-002: at wide allocations, mark/name/context remain one compact reading group. A short title's allocated width stays within 16px of its ink width, and adjacent identity gaps stay at most 16px, mirrored in RTL. Free width belongs to the non-interactive drag region. Existing narrow title allocation, Details metadata, focus order and terminal identity remain acceptance requirements. Base 0a2cddd includes merged PR #106/#108; correction branch gui/full-width-title-group.

## Centered identity clarification, 2026-10-14

User clarified that centered placement was correct; only internal mark/name/branch spacing needed correction. This supersedes the earlier leading-placement requirement. Preserve the compact identity gaps and short-label natural allocation from PR109, but return the identity to HeaderBar's native center title widget. At wide widths, the identity group's midpoint stays within 24px of the content header midpoint; do not measure against the whole window including the workspace sidebar. Retain narrow title minima, attention, native drag region, keyboard order, metadata and terminal identity. Base bc39d5b includes merged PR109; branch gui/centered-workspace-identity. No unclarified requirement remains.
