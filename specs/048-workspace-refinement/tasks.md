# Tasks: Workspace visual refinement

## Setup
- [x] T001 Research references and record bounded acceptance in specs/048-workspace-refinement/spec.md and research.md.

## US1 — Calm workspace scan
- [x] T002 [US1] Add red generated/explicit/timed headline regressions in crates/signaltty-gui/src/sidebar.rs.
- [x] T003 [US1] Implement provenance-aware headline hiding and decorative branch/directory image in crates/signaltty-gui/src/sidebar.rs.
- [x] T004 [US1] Unify row/search/footer geometry and simplify selected surface in crates/signaltty-gui/data/style.css.

## US2 — Native appearance and sizes
- [x] T005 [US2] Extend existing display regression for metadata and refresh cases in crates/signaltty-gui/src/sidebar.rs; inspect light/dark/narrow/large/high-contrast native renders.

## Delivery
- [x] T006 Run full verification and independent diff/visual review; document evidence in specs/048-workspace-refinement/verification.md and docs/06-gui-toolkit.md.
- [ ] T007 Commit and open linked PR with sources and proof.

Dependencies: T001 → T002 → T003 → T004 → T005 → T006 → T007.
Independent review can run alongside full verification after implementation.
MVP is US1; US2 is required acceptance before submission.
