# Tasks: Sidebar app identity

## Setup
- [x] T001 Read repository/design workflows and primary guidance; record research.md.
- [x] T002 Complete spec.md, plan.md, data-model.md, contracts/ui.md and quickstart.md before code.
- [x] T003 Capture parent native header/icon baseline through app_tests.rs.

## US1 Sidebar recognition
Independent test: native header geometry, names/focus/action, light/dark/minimum-width/enlarged/RTL scenes.
- [x] T004 Add and run red identity acceptance assertions in crates/signaltty-gui/src/app_tests.rs.
- [x] T005 Implement native static lockup in app.rs and scoped style.css.
- [x] T006 Inspect renders and exercise the retained New Workspace action.

## US2 Consistent desktop artwork
Independent test: bundled resource provenance and native regular/symbolic size matrix, isolated install tests.
- [x] T007 Replace both app SVG assets with original matching prompt/parallel-rail artwork.
- [x] T008 Run native size/resource matrix and scripts/tests/test_desktop_install.py.

## Delivery
- [x] T009 Update docs06/16 and .agents/skills/verify-signaltty/features/structure.md, read verify-signaltty skill.
- [ ] T010 Commit source; run full gate and independent Standards/Spec/visual review.
- [ ] T011 Commit selected proof and verification/handoff, publish/link stacked PR.

Dependencies: T001-T004 precede implementation. US1/US2 share artwork/header ownership and integrate sequentially. Independent review axes run in parallel under code-review skill. No parallel implementation writers.
