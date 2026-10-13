# Implementation plan: Sidebar app identity

Branch gui/sidebar-identity-polish | Base 29e9aa6, parent PR #106 | Date 2026-10-14 | [Spec](spec.md)

## Summary
A native horizontal brand lockup pairs a 20px symbolic mark with a 600-weight system-font wordmark. Keep existing header height and New Workspace action. Replace the original app artwork with a simple prompt plus two parallel rails, using the same geometry in the launcher tile and small symbolic mark. Mark follows text colors in the UI; the launcher alone carries a restrained cool brand hue.

## Technical context
Rust 2021/minimum 1.92, pinned 1.94.0, GTK4/libadwaita/VTE. Existing GtkBox/Image/Label/HeaderBar and GResource/icon theme. No dependencies or new state. Original SVG sources use filled paths for symbolic compatibility. Existing install helper copies matching scalable app assets. Native tests and existing Python installation seam; full gate with serial Rust scheduling on this contended host.

## Constitution check
Spec/plan/tasks before source. Baseline followed by red geometry/resource/accessibility assertions before implementation. Native toolkit ownership and semantic theme colors preserved; new icon color is confined to brand artwork. No architectural decision or ADR required. Full gate and independent two-axis/visual review. No deviations after design.

## Structure
App header in crates/signaltty-gui/src/app.rs; scoped branding CSS in data/style.css; app SVG assets in data/icons/scalable/apps/. Existing app_tests.rs native seams exercise the lockup and resource-size matrix. Existing desktop installation tests verify deployment. docs06/16 and verification feature map describe the presentation. Durable proof in docs/qa/sidebar-identity-2026-10-14/.

## Strategy
Capture parent shell scenes and old native icon artwork. Add a focused GTK identity test with geometry/focus/resource provenance assertions and capture hooks, run red. Implement only branding and two owned app assets. Inspect actual light/dark/200px/enlarged/native-HC/RTL renders and small-size icons, adjust weight/geometry if needed. Retain all terminal widgets and existing actions. Pin source for independent review and a clean full gate; publish the stacked PR and link immediately.

Parent PR #106 advanced to 29e9aa6 during the first gate. Integrated its compact-tool dismissal fix, preserving the docked sidebar. Documentation conflict retained both the updated toolbar map and new branding map. Source baseline captures remain f8c3f18; the final full gate follows this integration.
