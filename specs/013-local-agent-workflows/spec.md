# Feature Specification: Local agent workflows

**Feature Branch**: `013-local-agent-workflows`
**Created**: 2026-09-30
**Status**: Implemented and verified
**Input**: "browser e ssh no. Vai con le più importanti", following the cmux/herdr comparison and priority recommendation: approvals, daily GUI navigation, worktrees/Git.

## User Scenarios & Testing

### User Story 1 - Answer real permission requests (Priority: P1)

An agent requests permission to use a tool. The workspace shows the actual request and lets the user allow it once or deny it, with the answer reaching the agent's native permission mechanism.

**Why this priority**: The existing decision bar is not connected to automatic provider permission requests.
**Independent Test**: Execute an installed provider reporter with a documented native permission payload in a managed pane; answer through the normal UI/API and observe the provider verdict at the reporter's output.
**Acceptance Scenarios**:
1. Given a supported native permission channel, when a request arrives, its tool and input are visible and Allow once/Deny choices work without terminal input.
2. Given a provider without a proven native channel, the request is read-only and directs the user to its terminal.
3. Given an unanswered request, focus/reading preserves it. Timeout, disconnected reporter, supersession, exit or provider cancellation must never imply approval or retain actionable stale buttons.
4. Given concurrent answers, exactly one is delivered. A stale request cannot answer a newer request.

### User Story 2 - Navigate and inspect terminals (Priority: P2)

Users can find a workspace or command, rename the selected workspace, search terminal output and temporarily enlarge a pane without changing saved layout.

**Why this priority**: Frequent navigation currently needs sidebar clicks and a narrow set of shortcuts.
**Independent Test**: Open the actual GTK controls, search and select a workspace, rename it, find a known terminal marker and toggle pane zoom while checking terminal identity and saved ratios.
**Acceptance Scenarios**:
1. A searchable command/workspace palette is available by keyboard and menu; Escape dismisses it without consuming a terminal command.
2. Rename changes the workspace label while retaining its stable handle; empty names are rejected.
3. Terminal search treats ordinary text literally, supports next/previous matches, and reports invalid/unmatched input without crashing.
4. Zoom shows the active pane alone and restores its siblings/ratios when toggled off. Existing terminal widgets and processes survive.

### User Story 3 - Isolate work and review Git changes (Priority: P3)

Users create or open a Git worktree as a workspace, see its branch, inspect changed-file counts in the GUI and remove a worktree deliberately when finished.

**Why this priority**: Parallel agents need separate checkouts and the GUI needs visibility into their changes.
**Independent Test**: Use a temporary real repository to create/list/open/remove a worktree via CLI/API and inspect its real Git state and linked workspace. Exercise the GUI entry points and diff display.
**Acceptance Scenarios**:
1. Creating a worktree creates a separate checkout and linked workspace; opening a registered checkout reuses an already open workspace.
2. Listing shows actual registered worktrees, branch and open workspace association. Existing externally created worktrees can be opened.
3. Removal refuses dirty worktrees, the main checkout and worktrees with running pane processes. It preserves the branch and affects only the selected worktree.
4. Closing a workspace alone leaves its checkout and any other workspace intact.
5. The GUI shows worktree-vs-HEAD changed-file and line counts and distinguishes binary/untracked files. It does not label these as per-turn changes.

### Edge Cases

- Provider hooks outside managed panes, missing identity, malformed inputs, server restart during approval, duplicate/superseded requests.
- Closed workspace while a dialog/request is open; late IPC completion after navigation; no selected pane; tiny split and light/dark themes.
- Non-repository, detached or unborn HEAD, worktree paths containing spaces, duplicate branch/path, failed Git operation, locked/prunable worktrees and multiple simultaneous requests.

## Requirements

### Functional Requirements

- **FR-001**: Native permission requests MUST be represented as structured decisions automatically for proven providers, with explicit once-only Allow/Deny responses.
- **FR-002**: Automatic approval, permanent policy changes and guessed TUI keystrokes MUST NOT be introduced. Unsupported providers retain terminal approval.
- **FR-003**: Permission waiting MUST be bounded and must clean up cancelled/stale decisions without granting access.
- **FR-004**: Existing nonpermission hook reporting and owned configuration preservation MUST continue working.
- **FR-005**: GUI MUST provide command/workspace search, rename, literal terminal search and temporary pane zoom, accessible through native controls and shortcuts.
- **FR-006**: Navigation and zoom MUST preserve terminal widgets, process identity and saved layout.
- **FR-007**: Worktree operations MUST be available to CLI/API clients and creation/opening to GUI users; actual Git registrations are authoritative.
- **FR-008**: Removal MUST refuse dirty/main/live-workspace targets, retain branches, and never cascade unrelated workspace closure.
- **FR-009**: GUI MUST show aggregate and per-file working-tree change counts using the existing diff data.
- **FR-010**: New state and methods MUST retain stable handles, typed IPC validation, paired events and restart persistence where appropriate.

### Key Entities

- Native permission request: a transient provider-owned decision, waiting reporter and bounded lifetime. Restoring structure cannot restore its response channel.
- Worktree association: a workspace's canonical repository checkout; Git owns checkout registration and branches.
- Pane zoom and terminal search: client view state, independent of saved server layout.

## Success Criteria

- **SC-001**: Installed native permission reporters receive the exact selected provider verdict through real managed PTYs; all cancellation cases produce no Allow verdict.
- **SC-002**: GUI palette, rename, search and zoom operate without recreating any VTE terminal or mutating split ratios.
- **SC-003**: Real-Git integration checks prove checkout isolation, open reuse, dirty/main/live removal rejection and branch retention.
- **SC-004**: Workspace build, formatting, Clippy and tests pass with no new warnings; relevant GTK tests and light/dark inspection pass.

## Assumptions

- User authorization covers local implementation and the listed priorities. Browser and managed SSH are explicitly excluded.
- Complete these three useful workflows; provider proliferation, PR network integration, listening-port discovery, per-client seen state, per-turn transcripts/diffs and persistent Always policies are separate work.
- Native provider availability determines support, with documented response-channel verification and honest read-only fallbacks.
- Existing project-approved testing seams are core unit tests, server/CLI integration with temporary PTYs/repos, and GTK display tests where actual widgets matter.
