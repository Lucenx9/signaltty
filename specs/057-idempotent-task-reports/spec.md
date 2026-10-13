# Feature Specification: Retry an accepted task report

**Feature Branch**: `orch/idempotent-task-reports`
**Created**: 2026-10-14
**Status**: Ready for implementation
**Input**: Improve orchestration after verified PR readiness. Make an accepted worker report safe to retry when its response is lost.

## User Scenarios & Testing

### User Story 1 - Recover a lost report acknowledgment (Priority: P1)

A worker retries its result after losing the acknowledgment. It can confirm that the original result was accepted without changing the finished task.

**Independent Test**: Submit a report, reconnect, and submit the same content. Both calls succeed and the original result and timestamps remain unchanged.

**Acceptance Scenarios**:

1. **Given** an accepted completed, failed, or rejected report, **When** the same content is submitted again, **Then** the caller receives the current task with the original result.
2. **Given** two simultaneous identical reports, **When** both are processed, **Then** both succeed and observers receive only one result notification and one task update.
3. **Given** an accepted report restored from a saved snapshot, **When** it is retried, **Then** the original result remains accepted without a new notification.

### User Story 2 - Preserve the accepted outcome (Priority: P2)

A coordinator can trust that retry handling never replaces the worker's accepted outcome.

**Independent Test**: Change each report content field in turn and verify refusal with no task or event change.

**Acceptance Scenarios**:

1. **Given** an accepted report, **When** status, summary, ordered artifacts, or evidence changes, **Then** the report is refused and the original task remains unchanged.
2. **Given** a terminal task without an accepted result, **When** a report arrives, **Then** the existing terminal-state refusal remains in effect.
3. **Given** an accepted report and a later review disposition, **When** the original report is retried, **Then** the current disposition and task metadata remain unchanged.

### Edge Cases

- Omitted optional content and its existing decoded default are equivalent.
- Evidence object key order is irrelevant; artifact order and string contents are significant.
- Server-assigned report time is excluded from content matching.
- Pane selection uses the task's retained association, which can survive pane closure. Explicit task ID remains the stable selector.
- Malformed reports and missing task identities retain existing validation errors.

## Requirements

### Functional Requirements

- **FR-001**: An identical report for an already accepted result MUST succeed and return the current task.
- **FR-002**: Content matching MUST include status, summary, ordered artifact values, and decoded evidence values, excluding the server-assigned report time.
- **FR-003**: A retry MUST preserve the accepted result, its original report time, task update time, lifecycle, disposition, and other task metadata.
- **FR-004**: A retry MUST emit zero result or task-update notifications. The first accepted report MUST retain the existing paired notifications.
- **FR-005**: Different content, canceled tasks, and other terminal tasks without an accepted result MUST remain refused.
- **FR-006**: Identical concurrent reports MUST converge to one accepted result and one pair of notifications.
- **FR-007**: Matching MUST work on saved results restored through the existing snapshot mechanism without new stored fields.
- **FR-008**: Existing selectors, optional-field defaults, validation limits, and error types MUST remain in effect.

### Key Entities

- **Task report**: Worker-supplied outcome content and a server-assigned acceptance time.
- **Accepted result**: The immutable outcome attached to the task, including its original acceptance time.

## Success Criteria

- **SC-001**: Ten retries of each accepted outcome succeed, preserving the original result and timestamps with zero additional notifications.
- **SC-002**: Every tested content mismatch is refused without altering the accepted task or notifying observers.
- **SC-003**: Two concurrent identical submissions produce one accepted result and exactly one result notification and task update.
- **SC-004**: A retry after saved-state restoration succeeds with the original result and no additional notifications.

## Assumptions

The worker retains the task ID and the original report content. This change confirms an accepted result while it is available in current or restored state. It does not add a synchronous durable-write acknowledgment, coordinator delivery, automatic retry policy, or task reopening. Existing core and IPC test interfaces are the agreed verification boundaries. No unresolved clarification remains.
