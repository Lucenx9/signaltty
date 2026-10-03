# Feature specification: Reliable events and new-work waits

**Feature Branch**: `t3code/review-cmux-hedr`
**Created**: 2026-10-03
**Status**: Clarified
**Input**: "Ok, procedi e manda PR", following the cmux/Herdr source review.

## User scenarios and testing

### User story 1: Recover event delivery after restart or overload (P1)

As a client or agent consuming workspace events, I can resume from my last
cursor without confusing old events with new ones. If history is unavailable,
I receive an explicit recovery instruction instead of an incomplete success.

**Independent test**: Publish events, restart the server, publish again, and
resume from a cursor. Force retention overflow and verify a declared loss.

**Acceptance scenarios**:

1. Given an event cursor from before restart, when new events are published
   after restart, then they remain distinguishable and replayable.
2. Given a cursor outside retained history or a replay larger than its bound,
   when a client subscribes, then the reply declares incomplete history.
3. Given a slow client and excessive terminal output, when event delivery loses
   data, then the connection declares loss and the GUI recovers current
   workspace state and terminal screens.
4. Given a current terminal widget, when recovery replaces its screen, then its
   widget identity and focus remain stable, with no duplicate output bytes.

### User story 2: Wait for new agent work (P1)

As an orchestrating agent, I can submit work to a reused pane and wait for a
completion or blocked state newer than my captured baseline. The old completed
turn cannot satisfy that wait.

**Independent test**: Put an agent in Done, capture its baseline, begin a wait,
then publish Working and Done. Verify that only the new work satisfies it.

**Acceptance scenarios**:

1. Given a completed previous turn, when a new-work wait starts, then it remains
   pending until a newer matching lifecycle transition occurs.
2. Given work that completes before the wait request arrives, when its matching
   transition is newer than the baseline, then the wait completes immediately.
3. Given a wait for multiple outcomes, when the agent becomes Blocked or Failed,
   then the wait returns that matching outcome.
4. Given replacement of the pane's running process or reported agent session,
   when a new-work wait is pending, then it returns an explicit identity error.
5. Given a disconnected caller or server shutdown, when a wait is pending, then
   it releases its resources without waiting for the original deadline.
6. Given an existing current-state wait without a baseline, when the pane already
   matches, then it retains its immediate success behavior.

### Edge cases

- Missing, corrupt, rotated, truncated, or disabled audit history.
- Cursor ahead of this server, legitimate gaps caused by terminal output or
  event filters, concurrent events during subscription replay.
- Snapshot response racing with terminal output; replayed overlap; forward byte
  gaps; repeated lag during recovery; pane closed while reattaching.
- New work ending very quickly, repeated same-state hooks, no semantic hooks,
  session identity learned for the first time, and pane resume with the same ID.
- Invalid or mistyped wait baselines and state lists; empty outcome lists.
- Existing pending approvals remain pending during recovery and acknowledgment.

## Requirements

- **FR-001**: Event identifiers MUST remain unambiguous across server restarts.
- **FR-002**: Subscription replies MUST distinguish complete replay from history
  loss, retention expiry, and bounded replay truncation.
- **FR-003**: A client that falls behind MUST receive explicit loss notification
  or connection termination that forces recovery; silent continuation is forbidden.
- **FR-004**: The GUI MUST recover authoritative state and the visible terminal
  screen while retaining terminal widgets and existing acknowledgment rules.
- **FR-005**: Recovery MUST discard output already covered by its new snapshot
  and MUST detect forward byte gaps.
- **FR-006**: Callers MUST be able to require a matching lifecycle transition
  newer than a captured pane baseline, including when it precedes wait arrival.
- **FR-007**: New-work waits MUST validate process/session identity and support
  multiple alternative lifecycle or attention outcomes.
- **FR-008**: Existing current-state waits and ordinary subscriptions MUST remain
  usable without the new optional parameters.
- **FR-009**: Waiting MUST stop on client disconnect and server shutdown.
- **FR-010**: CLI, runtime schema, agent skill, and IPC documentation MUST expose
  the new recovery and wait contract consistently.
- **FR-011**: Memory and disk retention MUST remain bounded. No client recovery
  or failed mutation may trigger automatic resubmission of a mutation.

### Key entities

- Event cursor: identifies an observed point in the event stream.
- Replay coverage: retained bounds and whether a requested interval is complete.
- Work baseline: pane lifecycle transition and process/session identity captured
  before submitting new work.

## Success criteria

- **SC-001**: Before/after restart replay returns distinct new events with zero
  identifier collisions in the regression scenario.
- **SC-002**: Every exercised retention, replay-bound, or delivery loss produces
  an explicit recovery result, with no silent partial success.
- **SC-003**: The terminal recovery fixture renders its expected current screen
  with zero duplicate bytes and the same terminal widget.
- **SC-004**: A previous completed turn never satisfies a new-work wait; a newer
  matching transition satisfies it even when it occurs before wait arrival.
- **SC-005**: Identity replacement and disconnected/shutdown wait cases terminate
  within the regression harness deadline rather than their long wait timeout.

## Assumptions and clarification

- Implement the first two priorities from the source review in this PR. Detection,
  new provider question/plan bridges, and snapshot durability are separate work.
- Preserve the native GTK client and toolkit-free core. Reuse current real-socket
  tests and actor test seams; no new provider integration is required.
- Current screen recovery does not promise full disconnected scrollback history.
- The server remains one writer per state directory. Arbitrary processes do not
  survive server death; restoring events does not imply restoring processes.
- Clarification scan found no unresolved requirements. Optional contracts preserve
  existing callers, explicit loss replaces unsupported completeness claims, and
  the already-authorized local workflow defines scope.
