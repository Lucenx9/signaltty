# Implementation Plan: Compact workspace hierarchy

Branch: `gui/compact-workspace-hierarchy` | Date: 2026-10-13 | [Spec](spec.md)

## Summary
Move status to the workspace name line. Ordinary unselected roots keep a quiet location line; selected or attention-bearing rows reveal useful activity. Keep task chips and disclosure controls. Header context prefers branch, then agents, then a directory basename; full values remain in details and accessible text.

## Technical Context
Rust edition 2021, development toolchain 1.94.0, minimum 1.92. Existing gtk4/libadwaita/VTE bindings and semantic CSS. No new storage, dependencies, IPC or server work. Use the retained Sidebar Row widgets and App header labels. Existing native tests, Xvfb/D-Bus captures and `scripts/verify.sh full` provide verification. Selection and background reconciliation must preserve widgets and terminals.

## Constitution Check
Spec/plan/tasks before code; test-first native selection/header regressions. Native interaction, semantic colors and restrained geometry satisfy product directives. No core or protocol changes, new persisted state or load-bearing architecture decision. Full verification and a second independent reviewer required. Post-design check: all gates satisfied, no deviations.

## Project Structure
`crates/signaltty-gui/src/sidebar.rs`: row geometry and selection-derived visibility.
`crates/signaltty-gui/src/app.rs`: bounded header context and complete details label.
`crates/signaltty-gui/src/app_tests.rs`: header and native stress-scene verification.
`crates/signaltty-gui/data/style.css`: restrained row spacing and secondary header type.
`docs/06-gui-toolkit.md`: current presentation contract.
`docs/qa/compact-workspace-hierarchy-2026-10-13/`: selected native captures and verification summary.
Feature artifacts: spec, plan, research, data-model, contracts/ui, quickstart, tasks, handoff and verification.

## Implementation strategy
First retain baseline captures with identical populated fixtures. Add failing checks for selection-dependent previews and short/full header context. Then change layout in place, preserving all task chips, attention text and disclosure controls. Refresh visibility after selection and reconciliation using the existing row data. Inspect light/dark, long-name, large-text/high-contrast and RTL scenes before full verification and independent review. No global theme overrides or animation changes.
