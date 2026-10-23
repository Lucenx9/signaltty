# Tasks: Oversized terminal scroll counts

- [x] T001 [FR-001] [FR-002] Record reproducible failures at the public terminal interfaces in crates/signaltty-term/src/headless.rs.
- [x] T002 [FR-001] [FR-002] Saturate scroll-budget accumulation in crates/signaltty-term/src/headless.rs and pass the regressions.
- [x] T003 [FR-003] Verify subsequent ordinary output and run the complete signaltty-term suite.
- [x] T004 [FR-001] [FR-002] [FR-003] Complete independent reviews and prepare final source for full verification and the delivery gate before PR publication.

Tasks run in order; both regression cases share one source file and writer.
