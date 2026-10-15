# Tasks: reliable terminal interactions

- [x] T001 [FR-001] [FR-002] [FR-003] [FR-004] [FR-005] Clarify scope, trace ownership, compare independent designs and record ADR-0040 plus native limitations.
- [x] T002 [FR-001] [FR-005] Reproduce NUL loss at the realized VTE-to-actor boundary, implement the bounded byte adapter and verify complete payloads, refused truncated messages and exited panes.
- [ ] T003 [FR-002] [FR-005] Add failing native clipboard, shortcut and context-menu checks; implement terminal-local actions with selection/live guards, cancellable reads across resume and VTE-native paste.
- [x] T004 [FR-003] [FR-005] Reproduce lost/duplicated local history; implement ordered canonical screen reconciliation and verify repeated main/alternate snapshots plus existing actor overlap handling.
- [x] T005 [FR-004] [FR-005] Add failing per-pane text zoom checks; implement bounded scale and reset, verify restyle, layout/search retention and plain Ctrl ownership.
- [ ] T006 [FR-001] [FR-002] [FR-003] [FR-004] [FR-005] Update terminal docs and verification map, inspect native light/dark evidence and complete independent Spec, Standards and unsafe-boundary reviews.
- [ ] T007 [FR-001] [FR-002] [FR-003] [FR-004] [FR-005] Freeze reviewed source for full verification, fresh delivery proof, ordered commits and ready PR publication.

T001 blocks T002–T005. One implementation owner completes each red/green slice before the next. Independent review follows implementation. T007 marks source ready for the external full/delivery run; the report and PR record its actual outcome without later source changes.
