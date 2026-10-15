# Feature specification: compact Worktrees toolbar

**Feature Branch**: `gui/worktrees-toolbar-polish`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Second of three authorized GUI polish PRs, using T3 Code visual quality and native interactions, through merge.

## User scenarios & testing
### User story 1: recognize creation without truncated chrome (P1)
A user opens Worktrees and sees a compact, fully readable creation control beside the window title.
**Independent test**: Open the populated Worktrees window in light and dark at desktop width.
**Acceptance scenarios**:
1. Given the Worktrees title establishes context, when opening the window, then the leading action reads New without ellipsis or a filled accent surface.
2. Given a narrow window, then the same action can use its existing plus icon while keeping its complete accessible name and explanatory tooltip.
### User story 2: keep native actions reachable (P1)
A user can create, refresh, close and operate checkout rows with the keyboard after resizing.
**Independent test**: Existing dialog integration scenario at 360px, Sans 18 and restored desktop width.
**Acceptance scenarios**:
1. Given large text or a narrow window, then header targets stay inside the window and retain at least 28px hit targets.
2. Given width restored, then New returns without truncation; checkout actions and existing guardrails still work.
### Edge cases
No checkouts, unavailable or active checkouts, a long branch/path, pending/failing requests and repeated dialog opens retain existing behavior.

## Requirements
- **FR-001**: The creation control must read New at desktop width without truncation or a filled accent surface, using the dialog title for context.
- **FR-002**: Narrow mode must retain the plus action with its complete accessible name and tooltip; labels return at desktop width.
- **FR-003**: Native title, refresh, close, keyboard reachability, checkout actions and operation guardrails must remain usable in light/dark, narrow and enlarged-text layouts.
## Success criteria
- **SC-001**: Desktop native captures show an untruncated New action and fewer filled toolbar surfaces.
- **SC-002**: Header actions stay within the 360px window with at least 28px targets under default/enlarged text.
- **SC-003**: Existing create/open/remove confirmation and error scenarios pass.
## Assumptions
Keep native dialogs, bundled plus artwork, semantic theme colors, system typography and current actions. No operation, task, model or IPC change. Shell centered identity stays unchanged.
## Clarifications
2026-10-15: accepted batch authorizes three independent PRs and merges. Worktrees title supplies context for New; full Create Worktree name remains available to assistive technology. No unresolved scope question.
