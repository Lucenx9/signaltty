# Specification: Diff revision and filename collisions

**Created**: 2026-10-15
**Status**: Clarified
**Input**: Continue finding bugs, publish confirmed fixes and merge after checks.

## User story

As a user reviewing workspace, task or turn changes, I can see the complete change summary even when a repository contains a file named after a comparison revision.

## Requirements

- **FR-001**: Workspace changes against HEAD remain available when an untracked file has the same name as the HEAD commit ID.
- **FR-002**: Task changes against the recorded base remain available when a file has the same name as the base commit ID.
- **FR-003**: Turn changes remain available when a file has the same name as either comparison tree ID.
- **FR-004**: Summaries preserve tracked counts and untracked paths and do not alter the repository index.

## Acceptance scenarios

1. Modify a committed text file and create an untracked file named after HEAD. Workspace and task summaries report the modification and untracked file.
2. Capture a turn snapshot, create a file named after its tree ID and capture another snapshot. Comparing them reports the new file.
3. A file named after the current comparison tree ID also leaves the comparison available.

## Success criteria

Each affected summary passes a regression test with real Git, and full repository verification passes. The tests fail before the fix.

## Clarification

No user clarification is needed. Revision IDs and filenames belong to separate Git argument groups. This correction preserves existing diff semantics and IPC schemas.
