# Feature Specification: Workspace visual hierarchy

**Branch**: `main` · **Created**: 2026-09-30
**Status**: Implemented and verified
**Input**: Improve the application's sidebar, workspace and panes using current
design guidance and the animate, apple-design and emil-design-eng skills; push.

## User scenarios and acceptance

1. **P1 — Scan workspaces.** Messages and project metadata use the available row
   width. Status remains visible beside a separate close action. Hover/focus
   does not shift text. Long strings ellipsize with full-text tooltips.
2. **P1 — Orient within splits.** Pane headers are distinct from terminal output;
   inactive pane titles remain readable. Focus remains discernible when an
   attention ring takes priority. Output itself is never dimmed.
3. **P2 — Read workspace context.** The branch appears before a potentially long
   directory in the workspace subtitle; the full context remains in a tooltip.
4. **P1 — Keep native accessibility.** Light, dark, high contrast, enlarged fonts
   and 360px windows work. Keyboard navigation remains immediate and existing
   reduced-motion feedback continues to follow desktop settings.

## Requirements

- Preserve native GTK/libadwaita widgets, semantic colors and system fonts.
- Preserve priority ordering, callbacks, accessible names, selected workspace,
  mounted VTEs, approval choices and structural split reconciliation.
- No new animation, dependency, IPC method or persistence change.
- Use existing display tests and real isolated native scenes to verify this
  presentation-only change; no tests duplicating CSS constants.

## Success criteria

- Before/after native screenshots show wider activity/location lines and clear
  pane headers in both themes, without clipped controls.
- Narrow, enlarged-font and high-contrast captures remain usable.
- Existing GUI display regressions and workspace build/tests/Clippy/fmt pass.

## Clarifications and scope

The repository's native Linux direction and attention-first workflow resolve
the design scope. Existing GUI seams are retained. Glass materials, new
navigation, custom fonts and decorative movement are outside this refinement.
No unresolved requirements or architectural deviations.
