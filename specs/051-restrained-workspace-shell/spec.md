# Restrained workspace shell

Created 2026-10-12. Accepted user direction: extend sober T3 Code styling to the rest of the UI, preserving native interactions. Update open PR99 alongside features049/050.

## User scenarios

US1: Users navigate workspaces, tabs, terminals and Changes and see the same restrained geometry as the dialogs. Acceptance: shell headers/window controls, sidebar rows, tabs, terminal cards/actions/search, Changes scope controls, inline decisions, primary empty-state actions and owned menu use coherent compact corners and flat hierarchy.

US2: Work remains identifiable and actionable. Acceptance: selected workspace/tab, focused terminal, approval/danger, disabled actions and keyboard focus stay distinguishable in light/dark, narrow, large-text and high-contrast scenes. Native hit areas, terminal identity/geometry, drag/resize, text input, popup dismissal and shortcuts retain behavior.

## Requirements

FR001: Apply 6px control/pane corners and flatter elevation across the owned shell. Preserve dot/status colors and attention priority; reduce decorative pill/card treatment, not signal.
FR002: Scope toolkit internal overrides through explicit owned header/tab/scope/menu/search classes. No global external chooser/window theme override.
FR003: Keep existing layout, minimum targets, typography, semantic themes, persisted settings and all terminal/state/IPC logic unchanged. No new dependencies or motion.

## Success criteria

Native populated shell, tabs, Changes, search and approval scenes reviewed light/dark. Existing narrow/high-contrast, focus and terminal identity checks pass. Full verification and two independent review axes complete. Record limits for desktop-only integrations.

No clarification remains. Product direction native behavior and semantic themes remain binding; no constitutional deviation.
