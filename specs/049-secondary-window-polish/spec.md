# Feature Specification: Secondary window polish

**Branch**: `gui/secondary-window-polish`  **Created**: 2026-10-11  **Status**: Accepted
**Input**: Implement the five secondary-surface improvements accepted after native visual audit; submit a PR.

## User Scenarios & Testing

### User Story 1 — Read and navigate tasks (P1)
Users identify a task from its title and reach all five states in a narrow window.
Acceptance: desktop and 360px boards show up to two title lines; long titles retain a full tooltip; visible previous/next controls reveal overflowing states without activating a task. Existing task activation, scroll and focus preservation remain intact.

### User Story 2 — Choose a checkout (P1)
Users distinguish branch and checkout location and open or remove the intended checkout.
Acceptance: branch is the primary label, path and existing state are secondary; short lists do not reserve a large empty body; long lists scroll; unavailable actions retain explanations and confirmations.

### User Story 3 — Find commands and choose appearance (P2)
Users distinguish command results from workspaces and see a theme's application surfaces before selecting it.
Acceptance: palette has visible group headings that follow filtering and never become selectable targets; keyboard activation still selects the matching action/workspace; shortcuts are secondary. Each of five themes shows both variants with sidebar, selection and terminal-like lines; selection persists and appearance previews remain intact.

### User Story 4 — Inspect changed files (P2)
Users distinguish directory headings, filenames and change status at a glance.
Acceptance: grouped directories have a clear heading treatment; file/status typography stays readable at 320px and in both appearances; paths and stats remain unchanged.

## Requirements
- FR-001: Implement every acceptance scenario above without changing server contracts, stored preferences or terminal ownership.
- FR-002: Keep native focus indication, literal user text, accessible control names and existing unavailable-action descriptions.
- FR-003: All surfaces remain usable in light/dark, narrow allocation and enlarged text; semantic secondary text is restored in high contrast.

## Success Criteria
- SC-001: All five board states can be reached by visible controls at 360px.
- SC-002: A two-checkout dialog removes the previous large unused lower area, while many checkouts remain scrollable.
- SC-003: Filtered palette results retain correct group and target mapping; all five theme selections persist unchanged.
- SC-004: Native screenshots reviewed in light/dark; relevant regressions and full verification pass.

## Assumptions and scope
The audit's five surfaces define scope. Preserve existing modal presentation, native fonts, semantic colors and selection rings. Theme miniatures are illustrative application anatomy, not live terminals. No new dependencies, IPC, multi-window lifecycle or animations. No unresolved clarification. Existing model entities remain unchanged.
