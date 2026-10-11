# Implementation plan: Icon hierarchy polish

Branch `gui/icon-hierarchy-polish`, stacked base `637779bde0e9f41c11142f16ccb32ce72f6a8fec` from open PR100. Spec: [spec.md](spec.md). Native Rust/GTK4/libadwaita/VTE, existing versions/toolchain. No dependencies or protocol changes.

## Constitution check

Spec, plan, tasks before source edits. Existing behavior tests extended before changing placement/assets. Native light/dark, narrow/large/high-contrast and RTL checks, full verification and independent code-review axes. Styling alone uses reviewed native captures rather than implementation-mirroring tests. Server/client boundaries and structural VTE reconcile stay unchanged. No ADR needed.

## Audit and design

Apple toolbar guidance and GNOME header grouping favor related controls together, native components, recognizable symbols and contextual placement. Current Changes panel icon resembles split controls; Board grid is generic; New Tab is unlabeled and mixed into the trailing view/menu tools. Pane Close sits directly beside split controls. Details uses a different folder glyph and an edit icon for Changes. Sidebar metadata rescales16px artwork to14px. Worktree create/refresh occupy a separate row between explanation and list, increasing vertical noise.

Keep semantic colors, system type and restrained corners. Use the existing original 16px monochrome SVG family with consistent1.5px optical weight. GNOME recommends2px main strokes; preserve the established lighter family for the accepted T3-like visual direction, and verify16/32px native rendering rather than indiscriminately thickening every symbol. Only Settings1.4 normalizes to1.5. No copied Apple assets or additional icon pack.

## Implementation shape

- Original Changes SVG: diff document with plus/minus. Reuse its single role name in header, compact sidebar and Details.
- Board SVG: three-column kanban geometry, also used for board empty state. New Tab artwork uses a browser tab strip and plus sign, distinct from split rectangles. Add a terminal prompt SVG for agent/details and project empty-state consistency. Existing folder/branch/search/PR glyphs remain the family baseline.
- Main header: New Tab native ButtonContent labeled at desktop, `pack_start` after breadcrumb; compact breakpoints clear label and retain accessible name. Board/Changes in one native horizontal group with a small logical margin separating it from the menu. Hide whole group below760sp; compact footer behavior from052 preserved.
- Pane actions: existing paired split buttons plus a separated Close Pane target via logical start margin. Preserve28px targets, callbacks, focus/reveal and terminal geometry.
- Sidebar metadata16px. Details glyphs16px and decorative accessibility role; consistent Folder/Changes/Terminal. Copy stays native flat with scoped6px geometry. High contrast removes details-glyph dimming.
- Worktrees: dialog helper returns its HeaderBar rather than the body; header owns Refresh icon and Create Worktree icon+label. Existing callbacks, suggested action and row Open/Remove remain. Native ButtonContent with can_shrink retains recognizable Folder/Trash actions when enlarged text would otherwise force rows outside the dialog. Below480sp a native dialog breakpoint clears the action labels, retaining the icons and full accessible names/tooltips. Newly loaded row setters apply immediately and restore desktop labels on widening. Create uses the same breakpoint/shrinking behavior. Compact content gets a480px minimum height so wrapped guidance does not consume the entire scroll viewport; desktop content returns to natural height. Tooltip and accessible name explain Refresh.

## Verification seams

Extend existing toolbar navigation test for desktop New Tab label, middle/compact label removal, glyph identity, logical group placement, bounds and unchanged action behavior. Add one native owned-symbol render/resolution test with an isolated IconTheme search path,16/32px contact sheets light/dark/high contrast. Extend existing Details clipboard test and worktree navigation fixture to check local action placement. Extend pane keyboard-focus test for separation without recreating VTE or changing target allocation. Snapshot populated shell via existing worker fixture before/after; sidebar/footer, details, worktrees and contacts provide independent review material.

## Files

App/terminal/sidebar/details/board/workspace_dialogs and existing CSS/resource manifest. Original SVGs under data/icons/scalable/actions. Tests in app_tests.rs and existing terminal module. Documentation: docs06/16, feature map, selected QA evidence. No generic icon registry or parallel action routing.
