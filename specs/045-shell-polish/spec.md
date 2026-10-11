# Feature Specification: Calm workspace shell

**Feature Branch**: `t3/polish-gtk-ui-ux`
**Created**: 2026-10-11
**Status**: Implemented and verified
**Input**: Study T3 Code, Vercel, Linear and visually polished Rust applications; polish the native UI/UX and submit a PR.

## User Scenarios & Testing

### US1 — Find the workspace that needs me (P1)

When several agents are running, ordinary progress should recede and requests
should remain prominent. The selected workspace should have a structural cue
in addition to its background, including in high contrast.

Acceptance: Working and Done retain their words and indicators without a filled
badge. Approval, Input, Warning, Error, Waiting and Failed retain filled badges.
Selection has an inset leading marker without changing row or terminal geometry.
Existing priority ordering, close targets and keyboard focus remain usable.

### US2 — Start from an empty window (P2)

An empty window explains the next action rather than only reporting absence.
An empty workspace explains how to start a terminal. Each primary action shows
its actual keyboard shortcut and invokes the existing action.

Acceptance: The first screen says “Start with a project” and explains choosing a
folder and launching a shell or agent. A workspace without tabs says “Open a
terminal”. New Workspace and New Tab remain the primary actions. Their shortcut
hints fit at 360px with enlarged text; no redundant controls or fake content.

### US3 — Keep the chrome quiet (P2)

Board and Changes controls should read as secondary tools; the active Changes
toggle remains distinguishable. Terminal cards use restrained elevation rather
than stacked shadows; urgent pane rings keep their existing strength.

Acceptance: Desktop tools are flat at rest, reveal hover/press feedback, and use
an outlined checked state. Light/dark, custom accents and high contrast retain
readable text, selection and focus. Existing breakpoint behavior is preserved.

## Requirements

- FR-001: Preserve native navigation, approvals, PTYs and all existing actions.
- FR-002: Preserve desktop fonts, theme tokens, reduced motion and immediate keyboard feedback.
- FR-003: Compare three concrete visual directions before implementation.
- FR-004: Review native light/dark, narrow and enlarged-text renders; run the full verification gate.
- FR-005: Keep the change confined to GUI presentation and its verification/docs.

## Edge Cases and Assumptions

Long workspace names still ellipsize with complete tooltips. Urgency outranks
selection on panes. Keyboard close actions remain reachable. Native desktop
patterns take precedence over copying web layouts. No new bitmap asset is
needed; imagegen's code-native exception applies. Make Bot UI's webhook workflow
does not apply. Existing screenshots are evidence, not generated mockups.
No unresolved clarification is necessary for this authorized polish pass.

## Success Criteria

The two empty-state actions reach their existing dialogs/handlers. At 360px,
including Sans 18, action and shortcut bounds remain inside the window.
Native scene comparison confirms working status is quieter than approval,
selection remains visible and terminal cards have less chrome. Full verification
passes and a second reviewer checks the diff and native evidence against docs/17.
