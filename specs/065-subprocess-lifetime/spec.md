# Feature Specification: Shared subprocess lifetime

**Feature Branch**: `t3/improve-codebase-architecture`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Consolidate subprocess lifetime ownership, preserve operation policies, and publish a PR.

## User Scenarios & Testing

### User Story 1 - Stop canceled work completely (Priority: P1)

A user cancels a review or a checkout operation. Its subprocesses must stop rather than continue reading or mutating the checkout.

**Independent Test**: Start an operation with a child process, cancel it after the child starts, and verify the child cannot write a later marker.

**Acceptance Scenarios**:
1. Given a running operation with descendants, when its deadline expires or its caller is canceled, then its process group is stopped and the direct child is reaped.
2. Given a command whose leader exits while a descendant holds a pipe, when the operation expires, then the descendant stops without an indefinite wait.

### User Story 2 - Preserve command results and review limits (Priority: P1)

Users receive the same successful results, diagnostic errors and bounded previews after the refactoring.

**Independent Test**: Run existing file-diff, turn and worktree tests alongside command execution tests.

**Acceptance Scenarios**:
1. Given simultaneous output and diagnostics, when a command completes, then both streams and its exit status are returned without a pipe deadlock.
2. Given an oversized preview, when it exceeds the existing limit, then the existing unavailable result is returned and the command stops.
3. Given turn capture, when the index is copied and Git conversions run, then the user's index and established comparison semantics remain intact.

### Edge Cases

Missing executable; nonzero exit; one stream overflowing while the other stays open; cancellation after pipe EOF but before exit; caller cancellation during output; child exit with descendants retaining pipes; success without accidentally killing an unrelated process group.

## Requirements

### Functional Requirements

- **FR-001**: File review, turn snapshots, worktree operations and task commands MUST share ownership of subprocess lifetime and cleanup.
- **FR-002**: Deadline expiry, output-read failure and caller cancellation MUST stop the owned process group and arrange direct-child reaping, including when descendants retain pipes.
- **FR-003**: Callers MUST retain command construction, deadlines, established output limits, successful results and error classifications.
- **FR-004**: Existing preview limits, Git normalization, private-index isolation and merge recovery MUST remain valid.
- **FR-005**: The shared implementation MUST capture stdout and stderr concurrently and MUST NOT signal a process-group identifier after releasing ownership of its leader.

### Key Entities

- Command execution: one running command, its owned descendants, captured streams and exit status.
- Operation policy: the command, deadline and any stream limits owned by the calling operation.

## Success Criteria

- **SC-001**: All four selected command paths use one lifetime implementation; callers no longer manipulate process-group guards.
- **SC-002**: Real-process cancellation, deadline, pipe and stream-limit tests pass; existing operation tests pass.
- **SC-003**: Full repository verification and delivery proof pass before publishing the PR.

## Assumptions and clarification

The user accepted the subprocess candidate and authorized implementation and PR publication. Existing operation policies are authoritative. No new global output limits, protocol changes, PTY changes, transport abstraction or provider changes are included. Cancellation cleanup may finish asynchronously through the runtime's child reaper after synchronous group termination. No unresolved clarification remains.
