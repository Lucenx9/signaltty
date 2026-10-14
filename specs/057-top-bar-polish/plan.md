# Implementation Plan: Top bar polish

Branch gui/top-bar-polish | Base f3f42f2 | Date 2026-10-13 | [Spec](spec.md)

## Summary
Use native HeaderBar regions to separate leading navigation/create, an expanding title lane and trailing attention/views/menu. Put the existing Details button in the title lane alongside a non-interactive 24px drag space (12px compact). Hide the decorative workspace mark at the narrow content breakpoint; retain full identity through its accessible control. Refine group spacing only within the owned content header.

## Technical Context
Rust 2021, toolchain 1.94.0/minimum 1.92. Existing GTK4/libadwaita/VTE. No dependencies, persistence, server or protocol changes. Reuse current buttons, actions, Details popover and breakpoints. Native geometry/focus/action tests and retained-VTE assertions at the existing App seam; production-CSS renders in isolated Xvfb/D-Bus. Full gate with RUST_TEST_THREADS=1 on the already-observed contended host retains every check and timeout.

## Constitution Check
Spec/plan/tasks precede source changes; red native geometry checks first. Existing native toolkit/theme roles and attention loop preserved. No architectural ownership change or ADR needed. Independent two-axis/native visual review and full gate before ready PR. Post-design: no deviations or unresolved questions.

## Structure
crates/signaltty-gui/src/app.rs: native HeaderBar composition and content-width breakpoints, compact tool synchronization and mark wrapping.
crates/signaltty-gui/data/style.css: owned-header group spacing.
crates/signaltty-gui/src/app_tests.rs: title allocation, group order, attention, focus, retained terminal and capture assertions.
docs/06-gui-toolkit.md, docs/16-visual-references.md and verification feature map: current presentation and researched guidance.
Feature artifacts: spec/plan/tasks/research/data-model/contracts/ui/quickstart/handoff/verification.
Selected native evidence: docs/qa/top-bar-polish-2026-10-14/.

## Strategy
Capture the current header fixture before changing expected geometry. Extend existing tests to protect readable title space and complete groups under wide/narrow/RTL/attention/large-text conditions. Try the native flexible title lane, inspect renders and correct any unwanted centering or drag-space behavior. Preserve all existing controls and keyed terminals; no custom CSD or layout polling.

## Final adaptive design
Window breakpoints retain panel docking at 1100sp/760sp. A native content BreakpointBin uses 860sp for icon labels and 520sp for compact relocation, so an open Changes panel cannot squeeze labeled actions into the title. New Tab joins the labeled compact sidebar tools. One weak-reference callback shows that footer when the sidebar is collapsed or the header creation control is hidden; breakpoint setters do not compete over footer visibility. The attention button hides directly, eliminating the concealed Revealer's measured 35px footprint.

Main advanced with #105 during verification. Integrated f3f42f2 before final gate; renumbered this feature to 057 because main now contains 056-pr-readiness. Baseline captures retain their original 9d1913a provenance.

## Full-width correction plan
Add native 1280/2400px short-title LTR/RTL capture and ink/allocation/group-gap assertions before code. Move expansion from identity labels/button to the blank drag spacer, retaining the native title lane and original narrow minima. Inspect actual wide and narrow captures; if native HeaderBar centering still adds an unwanted leading gap, refine native placement without custom window chrome. Extend current App-seam test, no new renderer or widget abstraction. Source pinned for full gate and two-axis review; publish a corrective PR on main 0a2cddd.

Final native placement packs the title lane after navigation in HeaderBar's leading region, with an empty center widget. The labels, identity box and Details button use natural width; only the trailing blank box expands. This also removes native center alignment's excess gap before the mark. Native gaps from creation to Details stay at most 24px. Narrow long-title allocation remains 120px, versus the old 25px baseline; active attention leaves 67px and required controls remain available.

The shared runner uses a 1600px monitor, which clamps requested 2400px windows. The wide test uses up to 2400px bounded by monitor width minus 80px, while the dedicated 2560px display captures actual 2390px windows. Assertions remain the same in both runs; no native case is skipped and no deadline is relaxed.

## Centered identity correction plan

Use the existing native HeaderBar title-widget with a naturally sized compact title lane. Keep mark/title/context widgets non-expanding; retain the blank 24px/12px native drag reservation and existing breakpoints. Replace the navigation-adjacency test with center-position acceptance at wide widths, keeping ink/gap assertions. Run red on merged PR109 before implementation, then focused display tests and actual native captures, full gate and independent two-axis review. Same feature, no new dependencies or custom layout abstraction.

Native measurements refined the placement: a drag reservation inside the title lane offsets the actual identity center and consumes its width. Pack the existing blank drag widget at the leading edge, independently of the centered title. Native strict centering gives 0px center offset at 1270px and 2390px; both compact breakpoints restore loose centering to preserve narrow title/attention allocation. Closing Changes is awaited until content takes the full split width before wide captures.
