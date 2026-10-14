# Tasks: Organized agent workflow

## Setup and design

- [x] T001 [FR-012] Record clarified spec, grounding, alternative designs and source ownership in specs/061-agent-workflow and ADR-0037.

## US1: Locate work and guidance

- [x] T002 [FR-001] [FR-002] Test and implement read-only CLI status/context with explicit Spec Kit selection in scripts/agent.py and scripts/tests/test_agent_workflow.py.
- [x] T003 [FR-003] [FR-004] [FR-005] Add canonical .agents/workflow.json, generated full-slug specs index and task-routing projection; reorganize docs/README.md.

## US2: Advance and resume

- [x] T004 [FR-006] [FR-008] Test and implement next, phase checks and exclusive-create handoff through the script CLI.

## US3: Preserve verification proof

- [x] T005 [FR-007] [FR-010] Test tracked/staged/new content freshness and changed-during-run behavior, implement shared verification_evidence.py and extend verify.py.

## US4: Keep organization consistent

- [x] T006 [FR-009] Test repository integrity and generated command divergence, integrate checks in the shared verification runner and setup-agent.sh.
- [x] T007 [FR-011] Extend evaluation task bank/protocol with workflow discovery and resume cases without claiming measured improvement.
- [x] T008 [FR-012] Document CLI/ownership/failure recovery and align AGENTS.md, docs15/17 and canonical verification skill.
- [x] T009 [FR-001] [FR-007] [FR-009] Run focused CLI flows, full repository verification and independent Standards/Spec review; record actual evidence.

T001 precedes implementation. T002/T003 precede T004/T006. T005 has separate file
ownership and joins T006 integration. T007/T008 follow the finalized interface.
T009 gates publication. Root owns repository tooling/docs; evidence implementation
owns verification_evidence.py, verify.py and its own tests only.

## Verification record

The initial full run at 3ece6e9 passed with 45 isolated GTK tests and refresh
benchmark in target/verification/full-5zz9zs9s. Review findings were fixed
test-first in 84ded98; the 22 focused CLI/evidence cases passed. Standards and
Spec re-reviews closed all findings. The source recorded here is submitted to
`verify.sh full --output target/verification/agent-workflow-ci-ready-20261014`
before publication; delivery checks assess that report, not these checkboxes.

The first final gate stopped at the unchanged server test
`explain_reports_the_bundled_rule_that_matches` with a five-second working-state
timeout. The exact test passed alone in 2.23 seconds; all 24 tests in the targeted
module passed, retained in target/workflow-screen-recheck.log. No server source
was changed. Separate next-action regressions were fixed test-first so unavailable,
running and failed reports request inspection/diagnosis before another full run.

The reviewed source at a2df84d passed full verification and the delivery gate in
target/verification/agent-workflow-reviewed-20261014. Main then advanced through
PRs #113 and #114. They were merged without conflicts; the new full slug
060-cli-address-resolution was cataloged without colliding with 060-socket-startup.
The combined source requires the separate main verification report above.

The combined source passed full verification and delivery in
target/verification/agent-workflow-main-20261014. The first GitHub push exposed a
fresh-cache failure in existing offline Cargo packaging tests (run 38567064484).
An empty CARGO_HOME reproduced missing serde; locked metadata preparation made
both real packaging tests pass. The shared runner now prepares metadata before
offline tooling tests. A regression failed before the reorder and passed after it.
The CI-ready source uses the final report named above.
