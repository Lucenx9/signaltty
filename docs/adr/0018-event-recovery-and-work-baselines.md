# ADR-0018: Event recovery and work baselines

Status: accepted, 2026-10-03.

## Context

Scalar sequences reset while audit history survives. Replay silently truncates,
broadcast lag silently skips events, and PTY forward byte gaps corrupt the visible
screen. Current-state waits can return an earlier completed turn for new work.

## Decision

Reserve durable scalar sequence ranges before issuing numbers, skipping unused tails
on restart. Sync state-event audit records before publishing them; persist retention
before discarding a generation. Persist expected file lengths before publication;
validate loss evidence before rotation. Missing/truncated or uncertain legacy history
cannot prove completeness. Journal failure stops issuance rather than claiming
complete history. Publish through Store while holding its assigning lock.

Capture replay and its handoff fence together. Return either complete filtered replay
or explicit recovery metadata with no partial prefix. Lag/out-of-order delivery closes
the stream. Existing GUI reconnect refreshes state and replaces screens in retained
terminal widgets; forward byte gaps use the same recovery. Reattach preserves size.

Add an optional runtime work baseline from pane.get to wait. It identifies the pane's
process, known agent session and separate lifecycle/attention transitions. Resume and
restart invalidate process identity; exit does not. Preserve current-state waits.
Retain the last transition for each recognized outcome so brief matching states
remain observable after the pane moves on. Multiple outcomes and EOF/shutdown
cancellation support agent orchestration.

## Consequences

Unused sequence holes are legitimate. Current screen recovery does not restore offline
scrollback. Core and snapshot schemas stay unchanged. New-work waits cannot attribute
concurrent input writers; callers serialize those submissions. Audit IO is stricter
and incurs sync latency for state events; PTY output does not journal per chunk.
