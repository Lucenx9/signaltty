# Specification: Oversized terminal scroll counts

**Created**: 2026-10-23
**Status**: Clarified
**Input**: Find small reproducible bugs, publish fixes and merge after verification.

## User story

As a user supervising agent terminals, I can continue reading a pane after its
process emits scroll commands with exceptionally large numeric parameters.

## Requirements

- **FR-001**: Oversized scroll commands followed by other scroll commands or line-motion controls in one output chunk MUST NOT interrupt terminal processing through arithmetic overflow.
- **FR-002**: Printable text following an oversized scroll command MUST remain readable, including in a one-column terminal.
- **FR-003**: Subsequent ordinary output MUST remain readable through both screen snapshots and rendered reads.

## Acceptance scenarios

1. Feed a scroll-up command with the largest machine-sized unsigned count, another scroll command or line-motion control and a visible marker. Both read interfaces return the marker without a panic.
2. Feed an oversized scroll command followed by text into a one-column pane. Both read interfaces return the text without a panic.
3. Feed another ordinary text update after either scenario. Both read interfaces return the updated text.

## Success criteria

The regressions fail before the correction and pass afterward. The full terminal
test suite and shared repository verification pass.

## Clarification and scope

The requirements follow the authorized bug hunt and existing rendered-read
contract in spec 018. No product choice requires clarification. This correction
changes scroll-budget arithmetic only; protocol, persistence and rendering
semantics remain as documented in docs/05. No new architectural decision is needed.
