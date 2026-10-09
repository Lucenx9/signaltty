# Specification: Preserve an open task board during updates
**Branch**: t3/board-live-refresh | **Created**: 2026-10-08
**Input**: Continue native UI/UX quality; publish and merge all focused PRs.
## User story1 (P1)
While inspecting a scrolled board, background task events update its data without closing/reopening it or losing the user's place.
Acceptance: Same dialog and same task row survive metadata changes. Horizontal and vertical position remain; insertion/removal above a scrolled viewport preserves its surviving top card. At the top, new tasks remain visible. Moving a focused task follows its identity to its new column and reveals it; unrelated movement does not recenter. Removed focused task uses adjacent surviving row or native dialog focus. Closing before async refresh completes leaves it closed.
## Requirements
Key rows by task ID, use current pane destination on activation, update counts and conditional Done subset notice. Empty/nonempty transitions keep dialog identity. No new IPC/storage/dependencies/motion; terminal widgets unchanged.
## Success criteria
Production App regression first reproduces dialog replacement. Native tests then prove dialog/row identity, updates, focus, scroll anchors, movement/removal, pane target changes and empty transitions; existing navigation tests and full gate pass. Inspect light/dark renders before merge.
## Clarifications
Grok scope review identified that revealing Needs you before fixing replacement would recenter on each event. Therefore this PR precedes initial-attention reveal. No architectural deviation; native widget reconciliation follows existing GUI direction.
