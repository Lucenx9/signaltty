# ADR-0012: Preserve decisions on acknowledgment and keep GTK IPC asynchronous

**Status**: Accepted (2026-09-29) · **Spec**: `specs/008-qa-reliability/`

## Context

Focusing an approval currently clears its decision before the user can answer. GTK also waits synchronously for IPC replies, so a stalled server freezes the entire window. Reconnect discards terminal snapshots while keeping the existing widgets.

## Decision

`pane.mark_seen` and default attach acknowledge ordinary attention but preserve an unanswered decision and its required attention. Only an answer or an agent transition resolves a decision. This corrects the acknowledgment wording of ADR-0008 and spec 003; no optional destructive acknowledgment flag is added.

GTK uses local asynchronous request futures backed by the existing actor/oneshot channel. Socket operations have deadlines; a lost or timed-out mutation is reported and never automatically retried. Initial and reconnect snapshots enter the ordered UI stream and replace the screen of the persistent VTE widget. The server atomically captures a cumulative PTY byte
offset with its screen snapshot and includes the end offset in streamed chunks.
A viewer registers before the final snapshot; the client skips or trims chunks
already covered by that snapshot. These additive IPC fields close both output
gaps and overlap during concurrent attachment.

## Consequences

Blocked approval counts remain visible after reading. Clients no longer need a special focus guard. GUI actions and refreshes must capture owned IDs before awaiting, and cache refreshes must serialize without holding mutable GTK borrows. Reconnection repairs the current terminal screen rather than recreating widgets.
