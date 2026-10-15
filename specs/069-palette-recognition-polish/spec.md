# Feature specification: recognizable palette results

**Feature Branch**: `gui/palette-recognition-polish`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Third of three GUI polish PRs, following T3 Code visual quality, through merge.

## User scenarios & testing
### User story 1: recognize the workspace (P1)
A user opening the palette recognizes a workspace by the same mark used in the sidebar and content header.
**Independent test**: Open the palette with several workspace names in light and dark; compare marks to the sidebar.
**Acceptance scenarios**:
1. Given a workspace, then its result has the same ID-derived tint and name-derived monogram as its sidebar row.
2. Given similar names, then existing name/path text and search still distinguish the target; marks are decorative and never replace labels.
### User story 2: scan commands (P1)
A user scans commands by a quiet symbolic prefix and readable label, with shortcut hints subordinate.
**Independent test**: Search Show Changes and New Workspace, activate existing targets and navigate with the keyboard.
**Acceptance scenarios**:
1. Given command results, then every result has a decorative, app-bundled symbolic icon from its action or context category.
2. Given filtering or keyboard selection, then the same target is selected and activation behavior remains unchanged.
### Edge cases
No matches, long workspace names and unbroken paths, 360px windows, enlarged text, high contrast and RTL keep required controls and text usable. A prefix does not take focus or announce duplicate text. No host icon theme is required for the new artwork.

## Requirements
- **FR-001**: Workspace results must reuse the stable tint and monogram of their sidebar identity, while retaining full name/path search and existing target mapping.
- **FR-002**: Command results must show a quiet, consistently sized decorative symbolic prefix, bundled with the app and appropriate to the action or its context category.
- **FR-003**: Result labels and shortcut hints must remain readable at narrow/enlarged-text sizes, with existing groups, no-match feedback, keyboard selection, focus and activation preserved.
- **FR-004**: Decorative prefixes must add no focus stop or duplicate accessible label, and follow logical leading placement in RTL and semantic high-contrast styling.
## Success criteria
- **SC-001**: Native desktop light/dark renders show matching workspace identity and command prefixes without extra filled command badges.
- **SC-002**: All command prefix resources resolve with external icon search paths removed.
- **SC-003**: Existing narrow, enlarged-text/high-contrast, search/target and keyboard-visible-selection scenarios pass with prefixes.
- **SC-004**: RTL render and allocation assertions place prefixes at the logical leading edge without changing target identity.
## Assumptions
Reuse existing bundled artwork and system typography; use category glyphs for actions without a dedicated existing glyph. Labels remain authoritative. No new icon family, font, dependency, animation, operation or IPC. Shell title centering stays unchanged.
## Clarifications
2026-10-15: user authorizes autonomous polish and three PR merges. Earlier Task Board #122 and Worktrees #123 are merged. Existing grouped palette stays; this slice adds recognition and alignment, not a new command model or ranking.

2026-10-15 CI clarification: wider system-font metrics at 360px with large text can truncate a command beside its shortcut. Compact rows must put the shortcut beneath the title; wider rows retain the trailing hint. Labels and shortcuts both remain visible, including after resizing.
