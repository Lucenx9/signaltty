# Tasks: CI and pull request workflow

## Setup

- [x] T001 [FR-001] [FR-002] [FR-003] Record observed PR115 duplication, clarified spec and primary-source design in specs/062-ci-pr-workflow/.

## US1 and US2: Verification without duplicate interference

- [x] T002 [FR-001] [FR-002] [FR-003] [FR-004] [FR-005] Update .github/workflows/ci.yml triggers, identity and credentials, preserving full/MSRV jobs and artifacts.
- [x] T003 [FR-006] Exercise invalid workflow lint, add pinned workflow-lint job and explicit .github/actionlint.yaml runner label, verify valid lint.

## US3: Reviewable delivery

- [x] T004 [FR-007] [FR-008] Add .github/pull_request_template.md, docs/19-ci-and-pull-requests.md and ADR-0038; connect docs17/18/index and workflow context.
- [x] T005 [FR-008] Enroll feature062 in .agents/workflow.json, regenerate specs/README.md and run repository/phase checks.
- [x] T006 [FR-004] [FR-006] [FR-008] Obtain independent Standards/Spec reviews and record the final verification procedure in specs/062-ci-pr-workflow/verification.md.

T001 precedes source writes. T002/T003 precede T004's final guidance. T005 precedes
T006. Root owns all source writes; reviewers are read-only. A verified task list
is separate from actual delivery proof. No parallel source writers are needed.

After T006, run the final full/delivery gate, then publish and link the PR.
Publication is gated by actual report status, not these checkboxes.
