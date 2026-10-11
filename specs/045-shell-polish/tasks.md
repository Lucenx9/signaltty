# Tasks: Calm workspace shell

## Setup and design
- [x] T001 Inspect primary references and compare three sketches in research.md and exploration.html.
- [x] T002 Write spec.md, plan.md, quality checklist and quickstart.md before implementation.

## US1 — Attention hierarchy
- [x] T003 [US1] Capture baseline populated scenes using app_tests.rs native evidence seam.
- [x] T004 [US1] Quiet progress badges and add structural selection in data/style.css.

## US2 — First action
- [x] T005 [US2] Add failing native empty-state action/shortcut/geometry coverage in src/app_tests.rs.
- [x] T006 [US2] Refine StatusPage copy and add native shortcut hints in src/app.rs.

## US3 — Quiet chrome
- [x] T007 [US3] Flatten secondary header tools and simplify pane elevation in data/style.css.

## Verification and delivery
- [x] T008 Review desktop/narrow/large native light/dark renders and full verification evidence.
- [x] T009 Obtain independent docs/17 visual/diff review, record limitations in verification.md.
- [x] T010 Update docs/06, commit and open/link the PR.

Dependencies: T001–T002 precede code. T003 precedes style changes; T005 precedes
T006. T004/T007 touch the same stylesheet and execute sequentially. T008–T010
follow implementation. Independent reviewer starts once the diff and renders
are available; there is no parallel file ownership.
