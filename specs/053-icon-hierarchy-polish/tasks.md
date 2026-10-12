# Tasks: Icon hierarchy polish

## Setup

- [x] T001 Audit docs06/14/16, constitution, primary web sources and apple-design/emil-design-eng; specify specs/053-icon-hierarchy-polish/spec.md.
- [x] T002 Write design, role and placement contracts in specs/053-icon-hierarchy-polish/plan.md, research.md and contracts/ui.md.
- [x] T003 Capture unchanged native baseline and record in specs/053-icon-hierarchy-polish/handoff.md.

## US1: Recognize controls

- [x] T004 [US1] Add failing owned-symbol resolution/render regression in crates/signaltty-gui/src/app_tests.rs.
- [x] T005 [US1] Add original Changes/Terminal glyphs and refine Board/NewTab/Settings under crates/signaltty-gui/data/icons/scalable/actions/; register in resources.gresource.xml.
- [x] T006 [US1] Apply consistent roles in app.rs, details.rs and board.rs; inspect native light/dark16/32px contact sheets.

## US2: Correct scope and position

- [x] T007 [US2] Extend toolbar/worktree/pane regressions before edits in crates/signaltty-gui/src/app_tests.rs and terminal.rs.
- [x] T008 [US2] Place responsive NewTab by workspace context and group Board/Changes in crates/signaltty-gui/src/app.rs.
- [x] T009 [US2] Separate pane Close from the split pair in crates/signaltty-gui/src/terminal.rs without shrinking hit areas.
- [x] T010 [US2] Move window Refresh/Create into native header in crates/signaltty-gui/src/workspace_dialogs.rs; preserve row targets.
- [x] T011 [US2] Verify desktop/360px/enlarged/RTL placement and retained terminal identity in crates/signaltty-gui/src/app_tests.rs.

## US3: Quiet secondary hierarchy

- [x] T012 [US3] Use16px metadata/decorative details roles and restrained Copy styling in sidebar.rs, details.rs and data/style.css.
- [x] T013 [US3] Verify path clipboard, details Changes handoff and high-contrast glyphs in app_tests.rs.

## Delivery

- [x] T014 Update docs06/16 and .agents/skills/verify-signaltty/features/ for icon/position proof.
- [x] T015 Run scripts/verify.sh full, inspect native scenes and commit selected docs/qa/icon-hierarchy-polish-2026-10-12/ evidence.
- [x] T016 Run independent code-review standards/spec axes and fix accepted findings; record specs/053-icon-hierarchy-polish/verification.md.
- [ ] T017 Push and open/link PR against main after #100 merged; close specs/053-icon-hierarchy-polish/handoff.md.

## Dependencies

Setup and red tests precede source changes. Icon asset/role work comes before layout acceptance. Stories share app.rs/app_tests.rs, so one writer integrates sequentially. Independent review axes can run in parallel after the diff is fixed; GTK tests use isolated processes. No extension hooks configured. MVP is coherent role mapping; delivery includes the accepted position audit.
