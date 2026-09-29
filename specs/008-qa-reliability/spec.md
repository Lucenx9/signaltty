# Feature Specification: Workspace reliability fixes

**Feature Branch**: `main` (direct push requested by the user)
**Created**: 2026-09-29
**Status**: Clarified
**Input**: Fix all nine confirmed failures in `docs/qa/2026-09-29.md` and push to main.

## User Scenarios & Testing

### User Story 1: Open and answer an approval (P1)

Opening a blocked pane must preserve its unanswered approval and attention.
The user can then answer it once through the existing answer channel.

**Independent test**: real IPC hook → mark seen or attach → decision answer.

1. Given a pending decision, focusing, marking seen, or attaching preserves the decision and required attention.
2. Given that decision, answering delivers exactly one choice and clears the gate.
3. Given ordinary unread activity without a decision, marking seen clears attention as before.

### User Story 2: Keep the terminal usable through connection problems (P1)

The window remains interactive while the server stalls. After reconnection,
the terminal restores its canonical screen before applying later output.

**Independent test**: stalled real Unix connection plus GTK main-loop progress;
reconnect with output during outage plus subsequent live output.

1. While a request awaits a stalled server, menu/sidebar/window interactions still run.
2. A stalled connection produces a bounded timeout and reconnects without replaying a mutation blindly.
3. Output produced while disconnected appears in the terminal after reconnection.
4. Snapshot replacement updates the existing VTE widget and cannot erase later streamed output.

### User Story 3: Keep processes attached to valid workspaces (P1)

Malformed layout or spawn requests cannot create invisible or misparented
terminals. Failed process creation leaves workspace structure unchanged.

**Independent test**: real server integration tests and the existing QA script.

1. A tab from another workspace is rejected before any process starts.
2. Layout replacement must contain every pane of that tab exactly once.
3. Non-finite ratios fail and finite ratios remain within 0.05–0.95.
4. Closing a tab or workspace also removes legacy orphan panes owned by it.
5. Failed automatic spawn creates no tab, pane, or structural event.

### User Story 4: Present consistent window and process state (P2)

Relaunching presents the existing window. Restored panes show stopped state
consistently in pane headers, tabs, and workspace summaries.

**Independent test**: GTK activation/window count and pure presentation tests.

1. Repeated application activation leaves exactly one window.
2. Saved working/blocked lifecycle metadata cannot make a stopped process appear running.
3. A live sibling still determines a workspace's current activity.

## Requirements

- **FR-001**: Viewing an unanswered decision must not dismiss it or consume its attention.
- **FR-002**: GTK callbacks must never block waiting for IPC.
- **FR-003**: Requests have deadlines; reconnect retries connections, not prior mutations.
- **FR-004**: Reattach applies canonical snapshots in stream order before later bytes.
- **FR-005**: Spawn and layout methods enforce parentage, membership, uniqueness, and divider limits.
- **FR-006**: Failed spawn publishes no structure changes.
- **FR-007**: Close operations stop every owned pane, including invalid state created by older versions.
- **FR-008**: Application activation reuses the existing window.
- **FR-009**: Presentation derives current process activity from live state without destroying saved lifecycle history.

## Success Criteria

- All original QA reproductions pass after the fixes.
- The GUI continues processing an interaction within one second of a simulated server stall.
- Approval choices remain available after navigation and are delivered exactly once.
- Closing a workspace leaves zero processes owned by that workspace.
- Workspace tests, graphical regression tests, formatting, and static checks pass with no new warnings.

## Assumptions

The user approved the reported behavior fixes and existing test boundaries.
Paid provider requests and unrelated desktop integrations remain outside this fix set.
Acknowledgment is not an answer: `pane.mark_seen` now preserves a pending decision
and its gate, including when invoked through attach. This intentionally corrects
the earlier explicit-dismissal wording in spec 003 and docs 03/08.
