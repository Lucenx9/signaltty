# Feature Specification: Reliable workspace interactions

**Feature Branch**: `gui/reliable-workspace-interactions`
**Created**: 2026-10-12
**Status**: Accepted
**Input**: Implement the accepted UX audit: safe tab/pane closure, recoverable New Workspace errors, explanatory validation and discoverable compact Board/Changes navigation. Open a PR.

## User Scenarios & Testing

### User Story 1 - Protect running terminals (Priority: P1)

Users can close a tab or terminal without accidentally stopping active sessions.
**Why this priority**: An accidental close can interrupt ongoing agent work.
**Independent Test**: Close an active terminal/tab, cancel and confirm; verify the named target and affected sessions.
**Acceptance Scenarios**:
1. Given a running terminal, when close is requested, then a confirmation names it, explains that it will stop, and defaults to Cancel. Cancel or Escape sends no close request.
2. Given a tab containing running terminals, then the same protection applies to the whole named tab. Confirmation continues to target that tab if navigation changes.
3. Given only exited/restored terminals, or an empty tab, then it closes directly.
4. Given a failed close or remote removal during confirmation, then the current UI remains consistent and reports failure without closing another target.

### User Story 2 - Correct and retry workspace creation (Priority: P1)

Users see what prevents creation and can retry a failed operation without re-entering their choices.
**Why this priority**: This is the app's central setup flow.
**Independent Test**: Enter an invalid name/custom command, fail creation, fail command startup after creation, then retry successfully.
**Acceptance Scenarios**:
1. Given an empty manual name, unavailable folder or invalid custom command, then the relevant field has a visible explanatory message and Create is disabled; correction removes that message.
2. Given valid input, when Create or Enter is activated, then progress is visible, duplicate submission is blocked, and the dialog remains open until success.
3. Given failed workspace creation, then original choices remain editable with an inline explanation and Retry action.
4. Given successful creation but failed command startup, then the dialog explains that the workspace exists, preserves its identity, allows command correction and retries startup in the same workspace. Its folder/name choices cannot change during that retry.
5. Given cancellation before submission, then no workspace is created. During an in-flight request the dialog cannot dismiss and imply cancellation of a server operation. After failure it can dismiss normally.
6. Given successful retry, then the dialog closes and focuses the created terminal unless the user has navigated elsewhere in the meantime.

### User Story 3 - Find tools in a compact window (Priority: P2)

Users can find Board and Changes without knowing their shortcuts.
**Why this priority**: Frequently used tools should remain recognizable.
**Independent Test**: At 360px, open the sidebar and activate labeled Board and Changes entries; verify the intended views and Changes toggle state.
**Acceptance Scenarios**:
1. Given a narrow window, then labeled Board and Changes controls are available beside pinned sidebar tools, and only the workspace list scrolls.
2. Given a wide window, existing header controls remain the visible entry points and compact-only sidebar controls are hidden.
3. Given large text/high contrast, labels, errors, confirmation choices and keyboard focus remain readable and reachable.

### Edge Cases

- A confirmed target disappears, exits, or the user switches workspace while confirmation is open.
- A folder disappears after selection or a command has unmatched quotes/trailing escape.
- A request is slow, fails before creation, or fails after creation. No automatic retry on ambiguous connection failure.
- A partial workspace is closed elsewhere before retry; explain the failed target rather than create a duplicate.
- Lifecycle Idle or Done does not imply the process exited; use current process state for confirmation.

## Requirements

### Functional Requirements

- **FR-001**: Running tab/pane closures MUST require explicit confirmation with Cancel as default; inactive targets close directly.
- **FR-002**: Confirmations MUST retain the original target; cancellation MUST preserve sessions and terminal identities.
- **FR-003**: Failed closes MUST retain or reconcile visible state and explain failure.
- **FR-004**: New Workspace MUST explain invalid folder/name/custom command beside its controls and prevent invalid submission, including Enter.
- **FR-005**: Creation MUST preserve the form until success and prevent duplicate or misleadingly cancelled submission while pending.
- **FR-006**: Failure MUST preserve choices and expose Retry; partial creation MUST retry the command in the already-created workspace.
- **FR-007**: Compact windows MUST expose labeled Board/Changes controls without duplicating visible wide-window entry points.
- **FR-008**: Existing terminal keyboard semantics, selection, approvals, native focus, themes and server session ownership MUST remain intact.

### Key Entities

- Close target: terminal or tab with stable identity, visible name and current process state.
- Workspace draft: folder, manual/automatic name and first command, retained across failure.
- Creation attempt: ready, pending, failed before creation, failed after creation, or complete.

## Success Criteria

- **SC-001**: Cancellation sends zero close requests; confirmation sends one request for the original target.
- **SC-002**: Invalid input explains every blocking field and cannot submit through pointer or Enter.
- **SC-003**: Retrying command startup after partial creation creates exactly one workspace across both attempts.
- **SC-004**: Labeled Board and Changes remain reachable at 360px, including enlarged text.
- **SC-005**: Light/dark native captures and behavioral verification show readable progress/error/confirmation states with preserved terminal identities.

## Assumptions

- All live sessions merit protection, including idle shells. Exited/restored sessions do not need warning.
- Keep the accepted restrained T3-like styling and native Linux interactions.
- No new server methods, persistence, dependencies, automatic retries, undo or broader onboarding redesign.
- Existing tests and isolated GTK fixtures are the verification seam. Manual screen-reader/IME/OSK proof is reported separately.
