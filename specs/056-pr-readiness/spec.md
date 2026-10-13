# Feature Specification: Verified PR readiness

**Feature Branch**: `orch/pr-readiness`
**Created**: 2026-10-13
**Status**: Clarified
**Input**: Improve signaltty orchestration using the Agent Orchestrator investigation and open a PR.

## User Scenarios & Testing

### User Story 1 - Trust the merge queue (Priority: P1)

A user sees a PR as ready only when a recent observation proves it is ready for its observed commit.

**Why this priority**: The existing board can show draft, conflicting, or outdated PRs as ready.
**Independent Test**: Pure domain tests cover readiness, including the five-minute expiry boundary.

**Acceptance Scenarios**:

1. Given an open, non-draft PR with a known commit, passing or absent checks, satisfied review, and a clean merge state, when its observation is less than five minutes old, then its stage is Ready to merge.
2. Given a draft PR, missing commit, unknown mergeability, blocked merge state, future observation time, or expired observation, when it is displayed, then it is In review.
3. Given conflicts, failed checks, or requested changes, when displayed, then it is Needs you.
4. Given a new observed commit with pending checks, when refreshed, then the previous positive result is replaced and it is In review.

### User Story 2 - Recover from failed observations (Priority: P1)

A user or coordinating agent can see that a refresh failed and cannot mistake old success for current readiness.

**Why this priority**: Connectivity and authentication errors currently leave positive cached results untouched.
**Independent Test**: Existing isolated server tests use a fake source-control client and real IPC.

**Acceptance Scenarios**:

1. Given a ready PR, when refresh fails, then the previous facts remain inspectable, the failure is recorded, and readiness is withdrawn.
2. Given several PRs, when one refresh fails, then other PRs still refresh and the failed task is identified in the result.
3. Given overlapping observations, when an older request completes last, then it cannot overwrite the newer observation.
4. Given an open board with no new events, when an observation expires, then the card leaves Ready to merge within 30 seconds.

### User Story 3 - Share one coordination status (Priority: P2)

A coordinating agent reading task details sees the same stage as the board.

**Why this priority**: Orchestration decisions should not depend on a GUI-only rule.
**Independent Test**: Server task get/list and CLI output expose the domain-derived stage.

**Acceptance Scenarios**:

1. Given identical facts and evaluation time, when task details and the board are read, then they agree on the task stage.
2. Given a saved task without the new PR facts, when restored, then it loads successfully but cannot claim readiness until refreshed.

### Edge Cases

Unknown source-control enum values stay unknown. A partial or malformed response is a failed observation. Replacing a task's PR while a refresh is running discards the old response. Closed PRs, merged PRs, canceled tasks, and recorded dispositions preserve their existing Done behavior.

## Requirements

### Functional Requirements

- **FR-001**: Preserve the observed commit, draft flag, mergeability, merge state, successful observation time, latest attempt time, and latest refresh failure beside PR facts.
- **FR-002**: Ready requires all positive facts from one successful observation less than five minutes old, without a later refresh failure. Clean merge state must include satisfied repository merge requirements.
- **FR-003**: Missing facts, unknown facts, and future timestamps never establish readiness.
- **FR-004**: Derive task stage through one shared domain rule and expose it to the board and task readers.
- **FR-005**: Failed single refreshes retain their error response. Batch refreshes identify failed tasks and continue successful ones.
- **FR-006**: Persist observation changes through existing state transitions and events. Reject responses for replaced PRs and older observation attempts.
- **FR-007**: Reevaluate an open board's time-sensitive state at least every 30 seconds.
- **FR-008**: Verify behavior with pure domain, isolated IPC, and existing GUI test seams before opening a PR.

### Key Entities

- **PR observation**: Facts for one observed PR head, with successful and attempted observation timestamps and failure evidence.
- **Task stage**: Computed Working, Needs you, In review, Ready to merge, or Done. It is separate from persisted task lifecycle.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Every unsafe readiness case in the acceptance matrix avoids Ready to merge.
- **SC-002**: A failed refresh immediately withdraws a previous ready result without losing its facts.
- **SC-003**: Task readers and the board agree for the same facts and time.
- **SC-004**: An open board withdraws expired readiness within 30 seconds without a server event.

## Assumptions

The existing five-stage board and on-demand refresh remain the user workflow. Five minutes is the maximum age of a positive observation, not a guarantee that remote state has remained unchanged. Readiness never authorizes an automatic merge. Automatic feedback, report inbox delivery, provider replacement, and detached PTY hosts remain separately scoped projects.

## Clarification review

The current authorization and research resolve scope, ownership, failure behavior, compatibility, and verification seams. No critical ambiguity remains and no additional user questions are needed.
