# Tasks

- [x] T001 Ground upstream/local behavior and clarify spec.
- [x] T002 Compare independent designs, cross-judge and record synthesis/ADR.
- [x] T003 [US1] Prove restart and screen-gap regressions fail before the fix.
- [x] T004 [US1] Implement sequence continuity and honest bounded replay.
- [x] T005 [US1] Fence replay/live delivery and recover lag/output gaps in place.
- [x] T006 [US2] Prove old-Done baseline wait fails before implementation.
- [x] T007 [US2] Implement transition baselines, identity and multi-outcome waits.
- [x] T008 [US2] Cancel long waits on disconnect/shutdown and expose CLI contract.
- [x] T009 Update protocol/agent docs and verification coverage.
- [x] T010 Run full verification, inspect renders and review final diff.
- [ ] T011 Commit, push, publish and link PR; inspect hosted checks.

T002 gates production code. T004 precedes T005; T006 precedes T007. T009 uses final
interfaces. T010 gates T011. Root integrates changes; design/judge agents are read-only.
