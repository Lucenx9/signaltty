# Implementation Plan: Workspace visual refinement

**Branch**: gui/workspace-refinement | **Date**: 2026-10-11 | **Spec**: [spec.md](spec.md)

## Summary
Refine existing workspace navigation with quieter selection, coherent geometry,
a symbolic metadata indicator and removal of generated redundant state text.

## Technical Context
Rust 2021, minimum 1.92; GTK4/libadwaita; native Linux desktop.
No new storage or dependencies. Existing sidebar summaries and row reconciliation.
Testing: headless content tests before implementation, isolated GTK regression and
native light/dark/narrow/large text renders, full verification.
Scope: sidebar.rs + app stylesheet, docs/06 and selected evidence.
Performance: reuse existing rows; no timers or refresh work added.

## Constitution Check
Spec written and clarified before code. Existing GTK stack retained. Pure content
regression is red first; native rendering uses existing fixture. No new IPC,
model transition or architecture choice; ADR unnecessary. Full and screenshots
required. Pre/post design check: passes, no exception.

## Project Structure
- crates/signaltty-gui/src/sidebar.rs: headline visibility and metadata indicator.
- crates/signaltty-gui/data/style.css: row geometry, selection and metadata styling.
- crates/signaltty-gui/src/app_tests.rs: existing native shell/long-title matrix.
- docs/06-gui-toolkit.md: final presentation rules.
- specs/048-workspace-refinement/: acceptance, research, plan and tasks.

## Design
Platform font, body name at 600, secondary activity 0.9em, metadata 0.82em.
Semantic foreground alpha and accent from current theme; existing neutral canvas
(#ffffff/#111114 pane, #0c0c0e dark sidebar) retained. Selection 8% neutral fill
and 2px lateral foreground indicator; high contrast adds explicit outline.
One 8px radius for sidebar rows/search/footer. No new motion.

Root: mark + name + close / useful activity + status / icon + place + agents.
Child: name + close / useful activity + status; metadata remains hidden.
Names and tooltips unchanged. Status keeps its trailing position when headline
is suppressed; existing row spacing remains independent of text contents.

Alternative: align every detail under title via 28px indentation. Rejected:
would reduce useful metadata width and attention room in narrow navigation.
Alternative: accent-wash selection and more colored badges. Rejected: would
compete with the attention vocabulary.
