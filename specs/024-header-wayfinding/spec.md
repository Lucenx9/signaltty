# Specification: Shell workspace header wayfinding

**Branch**: `t3/header-wayfinding` | **Created**: 2026-10-08
**Status**: Implemented and verified
**Input**: Continue UI/UX review after merging PR39, following herdr/cmux and t3code visual quality.

## User scenarios and testing

### User story 1: Identify the current shell workspace (P1)
A user switches to a workspace with no agents and reads its location without hovering.
Acceptance: The header shows branch before directory when branch exists, otherwise directory alone. Agent workspaces retain the existing agent summary. Empty context has no separator. Closing the last workspace clears context and tooltip.

### User story 2: Keep header controls reachable (P1)
A user opens a long workspace name and path in a narrow window with enlarged text.
Acceptance: At 360px, labels ellipsize inside the window while sidebar, new-tab and menu controls remain visible. Full location remains available as a literal tooltip. Light/dark native themes remain unchanged.

## Requirements
- FR-001: Use existing agent summary when nonempty; otherwise branch · directory, with home abbreviation and whitespace-only branch ignored.
- FR-002: Show a separator only when the context label is visible and nonempty; startup and no-workspace state clear it.
- FR-003: Fit long labels and fixed header controls inside 360px windows at normal and enlarged text sizes.
- FR-004: Preserve terminal widgets, workspace switching, attention and native keyboard actions.

## Edge cases
Missing/blank branch, empty directory, shell-to-agent switching, no workspace, long literal names and paths, large fonts, narrow window.

## Success criteria
Native regression proves visible shell context, preserved agent summary, no dangling separator and cleared last-workspace state. Rendered controls stay inside actual 360px bounds in light/dark and enlarged text. Full verification passes.

## Assumptions and clarifications
This presentation correction uses existing snapshots and libadwaita. No state, IPC, dependencies, animation or architecture changes. At extreme 360px/Sans18, labels may reduce to ellipses; full pointer tooltip context remains available. Readable untruncated context at that extreme is outside this correction. Broader board and contrast candidates remain separate. No unresolved requirements or constitution deviations.
