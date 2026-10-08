# Implementation plan: Palette usability

## Summary and technical context

Rust 2021, minimum 1.92, pinned toolchain, GTK4/libadwaita. Improve only palette.rs, its native display tests and docs/06. Preserve the action registry and terminal/workspace callbacks. Use GTK ListBox scrolling to reveal selection while the search entry retains focus. Use native AdwStatusPage and GTK-formatted accelerator labels in native ActionRow subtitles for empty feedback and accelerator hints. Constrain titles/subtitles to bounded lines with literal tooltips. No CSS tokens or new dependency are necessary.

## Constitution check

Spec and this plan precede code. Write and run the production-widget regression before the fix. Isolated GTK is the correct seam for scrolling geometry and search focus. No core, server or protocol changes. Native light/dark and enlarged high-contrast evidence, existing workflow tests, and scripts/verify.sh full are required. No ADR or deviation is needed.

## Research decisions

The input is an existing desktop workflow. Retain system fonts, libadwaita semantic surfaces, quiet hierarchy and instant keyboard actions. Apply apple-design feedback/wayfinding and emil-design-eng restraint; frontend-design is subordinate to the explicit GNOME product direction. Scroll only enough to reveal the selected row; do not transfer focus or animate keyboard navigation. Before declaring a scrolling cause, run a deterministic regression against the actual key controller.

## Data and contracts

The deprecated GtkShortcutLabel is deliberately avoided without raising the minimum libadwaita version. Existing Choice gains the action registry's optional accelerator for display only. Targets and IPC contracts remain unchanged. No storage migrations.

## Verification

A native palette fixture with 40 workspaces drives its real key controller. Assert selected-row bounds inside the scroller, first match visibility after filtering, unchanged search focus, no-match guidance/recovery and Enter behavior. Capture wide and 360px light/dark plus Sans 18 high contrast. Existing fast-Enter workflow and full gate remain mandatory.

## Verification environment correction

The full runner exposed a 640×480 Xvfb distribution default that makes the existing wide worker-chip fixture impossible. Pin its test screen to 1600×1200 in scripts/verify.py, including the refresh benchmark. The isolated Cairo test fails on the default screen and passes at the explicit size. Prepare the sidebar test by showing the sidebar and waiting for mapping before requesting handle focus. No application layout behavior or assertions change.
