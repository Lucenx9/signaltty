# Tasks: Shared subprocess lifetime

- [x] T001 [FR-001] Ground existing command ownership and clarify the accepted scope.
- [x] T002 [FR-001] Compare design candidates and record synthesis and ADR.
- [x] T003 [FR-002] Run the failing real Git-filter cancellation regression before migration.
- [x] T004 [FR-002] [FR-005] Implement and verify the shared command lifetime module with real processes.
- [x] T005 [FR-001] [FR-003] [FR-004] Migrate all four callers and remove their obsolete lifetime code.
- [x] T006 [FR-003] [FR-004] Verify existing operation results, diff limits, normalization and private-index behavior.
- [x] T007 [FR-001] [FR-002] [FR-003] [FR-004] [FR-005] Complete independent review and prepare final source for full verification and PR publication.

## Order and delivery

T001-T002 precede implementation. T003 reproduces the caller regression before migration; T004-T005 implement the lifetime seam and migrate callers. T006 verifies operation compatibility; T007 prepares the reviewed source for delivery. One writer owns this checkout; independent reviews are read-only. Full verification, fresh delivery proof, commit, push and PR linking follow preparation. Their immutable outcome is recorded in the verification report and PR body, without editing verified source afterward.
