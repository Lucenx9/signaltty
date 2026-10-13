# Tasks: Compact workspace hierarchy

## Phase 1: Setup
- [x] T001 Pin main base and load design/verification workflows; record in specs/055-compact-workspace-hierarchy/research.md.
- [x] T002 Complete spec quality checks and plan artifacts in specs/055-compact-workspace-hierarchy/.

## Phase 2: Baseline
- [x] T003 Capture identical populated baseline scenes using crates/signaltty-gui/src/app_tests.rs and retain native renders in docs/qa/compact-workspace-hierarchy-2026-10-13/.

## Phase 3: US1, compact sidebar
Independent test: select/restore working rows, retain attention/task previews and disclosure, compare quiet-row geometry and row identity.
- [x] T004 [US1] Add and run red behavioral checks in crates/signaltty-gui/src/sidebar.rs.
- [x] T005 [US1] Move textual status into the title line and reconcile preview visibility in crates/signaltty-gui/src/sidebar.rs.
- [x] T006 [US1] Refine row spacing and inspect native scenes from crates/signaltty-gui/data/style.css.

## Phase 4: US2, header hierarchy
Independent test: branch/agent/directory contexts and full literal names/paths under narrow and enlarged text layouts.
- [x] T007 [US2] Update and run red header acceptance checks in crates/signaltty-gui/src/app_tests.rs.
- [x] T008 [US2] Implement bounded context and complete accessible/tooltips in crates/signaltty-gui/src/app.rs.
- [x] T009 [US2] Tune secondary header type in crates/signaltty-gui/data/style.css and inspect native light/dark/stress renders.

## Phase 5: Delivery
- [x] T010 Update presentation contract in docs/06-gui-toolkit.md and feature verification map in .agents/skills/verify-signaltty/features/.
- [x] T011 Complete full gate and two-axis independent review; record outcomes in specs/055-compact-workspace-hierarchy/verification.md.
- [ ] T012 Commit selected proof under docs/qa/compact-workspace-hierarchy-2026-10-13/, push and open/link PR; update specs/055-compact-workspace-hierarchy/handoff.md.

## Dependencies and parallel opportunities
T001-T003 precede implementation. US1 and US2 have independent tests but shared CSS and capture ownership; implement sequentially. Native display tests run in separate processes. Review axes run in parallel after final source commit.

## Strategy
Deliver US1 then US2 as one bounded refinement. Test-first for changed selection/header behavior; use existing fixtures rather than introducing a new framework. Full verification and reviewed native renders are the delivery gate.
