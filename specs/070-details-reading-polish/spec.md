# Feature specification: readable workspace Details

**Feature Branch**: `gui/details-reading-polish`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Continue visual polish with frontend-design, emil-design-eng, apple-design and Grok 4.7 High assistance; publish 1–2 PRs and bring them to merge. This slice improves workspace Details.

## User scenarios & testing
### User story 1: scan location and activity (P1)
A user opens Details and distinguishes branch, location and task state without reading one equally weighted block.
**Independent test**: Native Details light/dark render with branch, path, task and PR.
**Acceptance scenarios**:
1. Branch is the primary location label; path is secondary but readable.
2. Task title can wrap to two lines, and worker state remains separately visible rather than sharing the truncated title.
### User story 2: keep useful information in small windows (P1)
A user with long branch/path/task names can read state and operate Details with enlarged text.
**Independent test**: Narrow 360px, large/wider font and RTL native scenes, exact path copy and Changes activation.
**Acceptance scenarios**:
1. Complete branch and task text remain available through named rows/tooltips when visible titles truncate.
2. Copy Path retains the exact original path; Changes retains existing navigation and closes Details.
3. Existing PR number/status and loading/unavailable diff totals remain.
### Edge cases
No branch, no task, no PR, long unbroken names, detached HEAD, enlarged/high-contrast/RTL, and delayed git responses preserve existing data and operations.

## Requirements
- **FR-001**: Branch must be the primary location label and path secondary, with high-contrast readability retained.
- **FR-002**: Task title must wrap up to two lines independently of the complete worker-state label; full branch/task text stays available when truncated.
- **FR-003**: Native narrow/enlarged/wider-font/RTL presentation must keep state, PR context and actionable controls usable.
- **FR-004**: Clipboard content, Changes/Open Folder/PR targets, current asynchronous response guards and mounted terminals must remain unchanged.
## Success criteria
- **SC-001**: Light/dark native captures show distinct branch/path hierarchy and separate task state.
- **SC-002**: A long-task fixture preserves its complete visible state at 360px and enlarged text without changing copied path bytes.
- **SC-003**: Existing Details navigation/clipboard test passes, extended for narrow/large/RTL and task wrapping.
## Assumptions
Native popover, system font and semantic theme tokens remain. No new icon family, dependency, IPC or persisted state. Main shell repo/branch group stays compact and centered. One cohesive PR is sufficient unless the audit establishes a second independent improvement.
## Clarifications
2026-10-15: user authorizes design choices, fixes, publication and merge. Grok 4.7 High audits the current UI independently; its recommendations are evaluated against evidence and existing native constraints. No further interview is required.
