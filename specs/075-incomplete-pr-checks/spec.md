# Specification: Incomplete PR checks stay pending

Created: 2026-10-29. Status: Clarified.
Input: Audit application bugs, publish separate fixes and merge after verification.

## User story

A user reviewing tasks must not see Ready to merge when a returned check lacks a known successful outcome.

## Requirements

- **FR-001**: Nonempty check entries with missing, malformed or unrecognized completion facts remain pending rather than passing.
- **FR-002**: Completed SUCCESS, NEUTRAL and SKIPPED check runs and SUCCESS status contexts retain passing behavior. Explicit failures override pending entries; an empty list remains none.
- **FR-003**: The task stage stays In review when an otherwise ready PR contains an incomplete check.

## Acceptance and success criteria

Public core regression tests reproduce incomplete check entries being marked passing before the fix and pending afterward. A task-stage regression exercises the visible readiness consequence. Existing check and workflow tests, full verification and current CI pass before merge.

## Clarification and scope

ADR-0034 already requires conservative readiness for missing facts. No new product choice, persistence shape, IPC method, dependency or GUI layout is introduced. Existing core public functions are the accepted test seam. No unresolved clarification remains.
