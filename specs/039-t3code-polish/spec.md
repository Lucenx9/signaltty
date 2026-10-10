# Specification: t3code visual polish pass

**Branch**: `gui/t3code-polish` | **Created**: 2026-10-10
**Status**: Implemented and verified
**Input**: Compare the GUI with t3code (pingdotgg/t3code @ 98beed1a) for visual polish and ship the gaps that fit GTK/libadwaita, using the apple-design, emil-design-eng and frontend-design skills.

## Evidence
Seeded Xvfb captures (four workspaces: working, done, permission, plain shell; light and dark) against t3code's `index.css`, `ThreadStatusIndicators.tsx` and command palette. Four gaps were visible; others (header path, tabular numerals) were already handled.

## User scenarios and testing

### User story 1: A plain shell costs one line less (P1)
A user scanning the sidebar sees name and place for a shell with nothing to report.
Acceptance: With no explicit message and lifecycle `Unknown`, the row hides its headline line ("Running"); task chips or any message bring it back. Agent states (Idle, Working, Done…) keep their line.

### User story 2: Working breathes instead of spinning (P1)
Acceptance: Sidebar status slot and pane header show the same dot as every other state, accent-coloured and pulsing (opacity 1 → 0.35, 800ms alternate, strong ease-in-out) while working. Reduced motion holds the dot still. No layout shift between states.

### User story 3: Palette commands scan in one line (P1)
Acceptance: Command rows show the title with the shortcut as a trailing dim hint and no "Command" subtitle; rows are 40px minimum. Workspace rows keep a second line with the `~`-abbreviated path. Search still matches "workspace", "command" and paths. Shortcuts stay inside 360px windows.

### User story 4: Dialogs lift off the dark canvas (P2)
Acceptance: In dark appearance, floating dialogs keep libadwaita's light inner outline and gain a dark 1px outer edge plus a deep soft drop (`0 24px 72px -20px`). Light appearance keeps libadwaita's shadow.

## Requirements
- FR-001: `quiet_headline` (no message, lifecycle Unknown) hides the activity line unless task chips are visible.
- FR-002: Spinner widgets in `LifecycleIndicator` and `StatusSlot` become a dot with a `pulse` class; `RowStatus.spinner` is renamed `pulse`.
- FR-003: Palette uses an explicit per-choice subtitle; shortcut renders as an `AdwActionRow` suffix.
- FR-004: CSS only for dialog elevation; no new dependencies, IPC or state.

## Out of scope
Docked diff panel, sidebar pin/snooze/settle, grouping by repository, grain texture and backdrop blur (GTK has no `backdrop-filter`). The native tab-bar loading spinner stays.

## Success criteria
Unit tests cover the quiet-headline rule and pulse flag; the palette narrow-window GTK test still finds the shortcut inside 360px; `scripts/verify.sh fast` passes; before/after captures reviewed in light and dark.
