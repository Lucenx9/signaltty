# Specification: CLI output gap detection

**Created**: 2026-10-14
**Status**: Clarified
**Input**: Continue finding repository bugs, publish confirmed fixes and merge after checks.

## User story

A user attached to a pane must learn when terminal output has been lost instead of continuing with a corrupted screen. This completes the CLI side of specs/016-agent-event-recovery FR-003/FR-005 and docs/08-ipc.md.

## Requirements

- **FR-001**: If an output chunk starts after the snapshot/live byte cursor, stop attach with a nonzero error explaining output loss and asking the user to attach again. Do not write that chunk or advance the cursor.
- **FR-002**: Contiguous output and overlap with the snapshot retain their existing behavior, including no duplicate bytes. Legacy output without offsets retains existing behavior.
- **FR-003**: Restore terminal input mode on the error path. Preserve the running server-owned pane and do not replay input mutations.

## Acceptance scenarios

1. A snapshot through byte 6 followed by five bytes ending at 12 causes an explicit error before those bytes are displayed.
2. Five bytes ending at 11 append exactly once after that snapshot.
3. Nine bytes ending at 11 discard four covered bytes and append five bytes exactly once.
4. Exiting on an output gap restores canonical input mode.

## Success criteria

The real CLI controlling-terminal regression distinguishes lost, contiguous and overlapping output without timing races. Existing CLI behavior and full repository verification pass.

## Clarification

No user questions are required. The existing IPC contract already prohibits silent continuation. Explicit failure with a fresh attach is sufficient; automatic reconnect is outside this correction. No protocol, persisted data, dependencies or architecture changes are needed.
