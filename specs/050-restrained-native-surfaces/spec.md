# Feature Specification: Restrained native surfaces

**Created**: 2026-10-11 **Branch**: gui/secondary-window-polish **Status**: Accepted
Input: Reduce the GNOME/Adwaita visual imprint toward sober T3 Code styling while preserving native interactions. User selected this direction explicitly.

## User Scenarios & Testing
### US1 — Consistent workspace tools (P1)
Users open Preferences, command palette, Worktrees, Task Board, workspace details and New Workspace and recognize one application visual language.
Acceptance: controls and surfaces use smaller coherent corner radii, quiet outlined secondary actions, flatter lists, fewer elevated cards and a single clear selected/primary cue. Existing theme previews and task titles remain readable. The shell continues to use the established semantic palette.
### US2 — Native interaction remains trustworthy (P1)
Acceptance: keyboard activation, search, filtering, dismissal and focus restoration are unchanged; suggested/destructive/disabled actions remain distinguishable. Light/dark, narrow and large-text/high-contrast scenes remain usable. Native touch targets, adaptive sheets, switches and choosers retain their platform behavior.

## Requirements
- FR001: Apply a restrained visual treatment to the six application-owned surfaces above; styling must not affect external/native file choosers or undocumented third-party controls globally.
- FR002: Remove unnecessary raised-card and filled-button weight while keeping state, primary actions, danger and focus unmistakable.
- FR003: Preserve all existing behavior and semantic themes; no new toolkit, fonts, IPC, lifecycle or persistent settings.

## Success Criteria
- All six surfaces have reviewed native light/dark evidence with visible focus and primary actions.
- Existing narrow, keyboard, focus/identity and preference persistence checks pass; full gate passes.
- High-contrast cues survive without depending only on faint fills.

## Assumptions and scope
Update open PR99; feature049 behavior stays intact. Use existing native typography and semantic color roles. Product direction already permits application visual tokens through libadwaita patterns. No unresolved clarification or architectural deviation.
