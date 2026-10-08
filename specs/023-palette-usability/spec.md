# Specification: Command palette visibility and feedback

**Branch**: `t3/ui-ux-visual-review` | **Created**: 2026-10-08
**Status**: Implemented and verified
**Input**: Review UI/UX and visual quality against herdr, cmux and t3code, with Grok 4.7 on direct xAI, and submit a PR.

## User scenarios and testing

### User story 1: Follow keyboard selection (P1)

A user searches a large workspace list and moves through results using Up/Down while continuing to type in the search field.

Acceptance: The selected result stays visible, including results beyond the initial viewport. Search retains keyboard focus. Navigation at either end keeps the current result. Enter activates the selected workspace or command. Filtering selects the first match and reveals it immediately.

### User story 2: Understand search results (P2)

A user sees that a query has no matches and can correct it without leaving the palette. Command results show their existing keyboard shortcuts to make repeated workflows easier to learn.

Acceptance: No matches shows a clear title and guidance to try another command or workspace name. Clearing the query restores results and selection. Existing shortcuts are readable in light/dark and narrow windows, and commands without shortcuts reserve no extra space.

## Requirements

- FR-001: Keyboard selection must remain visible in a long result list without moving focus out of search.
- FR-002: Filter changes must reveal the first matching result.
- FR-003: No matches must show an explicit recoverable empty state and Enter must do nothing.
- FR-004: Commands must show existing shortcuts without changing their mappings.
- FR-005: Long names, paths and shortcuts must fit 360px windows and enlarged text, with complete names available through tooltips.
- FR-006: Navigation and feedback must remain immediate, respect native themes and high contrast, and preserve workspace activation and terminal focus restoration.

## Edge cases

No workspaces; hundreds of workspaces; long unbroken names/paths; filtered hidden rows; first/last selection; no matches followed by clearing; commands without shortcuts; high contrast and large fonts.

## Success criteria

- SC-001: After navigating beyond the initial viewport, the selected result lies inside the visible result area and search retains focus.
- SC-002: A no-match query shows guidance, activates nothing on Enter, and recovers on clearing.
- SC-003: Native renders in light, dark and enlarged high contrast stay within a 360px parent and show readable shortcut hints.
- SC-004: Original workflow tests and the full project verification gate pass.

## Assumptions

This corrects and clarifies an existing UI workflow. No new actions, protocol, state, dependencies or architecture are introduced. All requested product references are translated into native libadwaita controls. No unresolved requirements or constitution deviations.
