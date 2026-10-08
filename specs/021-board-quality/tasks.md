# Tasks: Board quality

## Setup

- [x] T001 Record requirements and implementation plan in specs/021-board-quality/spec.md and plan.md.

## User story 1

- [x] T002 [US1] Add and run a failing native narrow-window regression in crates/signaltty-gui/src/app_tests.rs.
- [x] T003 [US1] Wrap the existing columns in native horizontal scrolling in crates/signaltty-gui/src/board.rs.

## User story 2

- [x] T004 [US2] Improve metadata contrast and remove redundant dim styling and restore high-contrast headings in crates/signaltty-gui/data/style.css and src/board.rs.

## Verification

- [x] T005 Inspect light/dark and enlarged high-contrast native renders from crates/signaltty-gui/src/app_tests.rs and run scripts/verify.sh full.
- [x] T006 Record verification and update the board presentation in docs/06-gui-toolkit.md.
