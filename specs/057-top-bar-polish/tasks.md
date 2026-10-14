# Tasks: Top bar polish

## Setup
- [x] T001 Research primary toolbar guidance and record design decisions in specs/057-top-bar-polish/research.md.
- [x] T002 Complete spec/plan/contract/checklist in specs/057-top-bar-polish/.
- [x] T003 Capture current native baseline using crates/signaltty-gui/src/app_tests.rs.

## US1: Identity and groups
Independent test: native wide/narrow title width, complete metadata and non-overlapping functional groups.
- [x] T004 [US1] Add/run red title-allocation and group assertions in crates/signaltty-gui/src/app_tests.rs.
- [x] T005 [US1] Implement native title lane, leading group and narrow decorative-mark wrapper in crates/signaltty-gui/src/app.rs.
- [x] T006 [US1] Inspect native renders and refine owned-header spacing in crates/signaltty-gui/data/style.css.

## US2: Native/adaptive behavior
Independent test: attention, keyboard, RTL, breakpoint restoration and retained terminal identity.
- [x] T007 [US2] Extend and run App-seam interaction/stress checks in crates/signaltty-gui/src/app_tests.rs.
- [x] T008 [US2] Inspect light/dark/native-HC/large-text/RTL geometry and drag-space allocations from crates/signaltty-gui/src/app_tests.rs.

## Delivery
- [x] T009 Update docs/06-gui-toolkit.md, docs/16-visual-references.md and .agents/skills/verify-signaltty/features/structure.md.
- [x] T010 Run full gate and independent two-axis review; record in specs/057-top-bar-polish/verification.md.
- [x] T011 Commit selected proof under docs/qa/top-bar-polish-2026-10-14/, publish/link PR and update specs/057-top-bar-polish/handoff.md.

## Dependencies and strategy
T001-T004 precede implementation. US1 and US2 share App/CSS ownership and run sequentially; independent review axes run in parallel. Use existing native test seams and capture helpers. Changes remain a single frontend concern; full verification and inspected screenshots are the delivery gate.

## Full-width correction
- [x] T012 Reproduce and capture wide short-title dispersion, add red ink-width/group-gap checks in app_tests.rs.
- [x] T013 Move flexible width to drag space in app.rs, inspect wide/narrow/RTL renders and run native regression tests.
- [ ] T014 Update current GUI/map guidance, complete full gate and independent review, retain source/evidence, publish/link correction PR.
