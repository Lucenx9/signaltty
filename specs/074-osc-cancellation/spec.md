# Specification: Cancelled terminal control strings

**Created**: 2026-10-26
**Status**: Clarified
**Input**: Audit application bugs, publish separate fixes and merge after verification.

## User story

As a user supervising terminals, I receive notifications, titles and progress only
from completed control strings. An interrupted string cannot create false attention.

## Requirements

- **FR-001**: CAN and SUB cancel an unfinished OSC without publishing its notification, title or progress payload.
- **FR-002**: Bytes following cancellation are scanned normally, including a standalone BEL and a new valid OSC.
- **FR-003**: Cancellation behaves identically at every input chunk boundary, including while discarding an oversized OSC.

## Acceptance scenarios

1. An OSC 9 notification interrupted by CAN or SUB emits no notification; the following BEL remains a bell.
2. Cancelled title and progress updates leave the previous evidence unchanged; a later valid update still applies.
3. Every split of these byte streams gives the same result. Cancellation of an oversized incomplete string exits discard mode.

## Success criteria

The scanner regressions fail before the correction and pass afterward. All terminal
tests and the shared full verification pass. Independent Standards and Spec reviews
and current GitHub checks are settled before merge.

## Clarification and scope

The existing OSC/BEL contract in docs/05 and spec 045 supplies the expected behavior.
CAN and SUB interrupt terminal escape sequences, as documented in the xterm control
sequence reference. No product choice needs clarification. No IPC, persistence,
GUI layout or architectural changes are required.
