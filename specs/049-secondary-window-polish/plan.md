# Implementation Plan

## Summary and technical context
Rust GTK4/libadwaita GUI only; reuse existing keyed rows, palette row indices and preference persistence. No dependencies or ADR-worthy architecture change.

## Constitution check
Spec precedes plan/tasks/code. Behavior regression assertions precede implementation. Toolkit stays confined to GUI. Full gate and reviewed native light/dark evidence required.

## Design
System font and existing semantic foreground/surface/accent roles; no new palette. Board title occupies its own wrapped two-line row; state pill follows beside secondary metadata. Overflow-only previous/next buttons move the horizontal adjustment by a column. Preserve epoch-based restoration.
Worktree scroll uses natural height propagation capped at 360px, with a minimum for loading; branch is title, path plus existing status is subtitle, full path tooltip.
Palette uses ListBox row headers based on visible target category so row indices remain unchanged. Theme cards replace decorative gradients with compact paired miniature sidebars/terminal content, using literal preview variables scoped per theme to prevent active-theme bleed. Changes adds semantic directory and filename/status roles.

## Verification
Extend existing native tests for board wrapping/control navigation, palette filtered headers, Worktrees content size and branch/path, paired theme miniature structure and persistence. Existing full suite covers activation, focus restoration, stale responses, diffs and row identity. Review all touched captures; add narrow preferences capture if feasible. Real desktop screen reader/IME not claimed.
