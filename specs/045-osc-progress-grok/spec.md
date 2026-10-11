# Feature Specification: OSC progress and Grok detection

**Feature Branch**: `agent/osc-progress-grok`  
**Created**: 2026-10-11  
**Status**: Draft  
**Input**: Add OSC 9;4 and Grok support, then submit a PR.

## User Scenarios & Testing

### User Story 1 — Progress stays separate from notifications (P1)
A managed terminal emits progress. Users receive completion attention only when
an applicable agent rule establishes completion, never a notification containing
progress protocol text.

**Independent Test**: Feed progress and notification sequences through the
existing scanner and through a real managed PTY.

**Acceptance Scenarios**:
1. Given a pane emitting busy then idle progress, when Grok rules apply,
   then it becomes working then done with unread attention.
2. Given an ordinary terminal, when it emits progress, then no notification or
   unread attention is raised by that progress alone.
3. Given split or malformed progress sequences, when bytes arrive, then valid
   sequences are retained and malformed progress is ignored without becoming a bell.

### User Story 2 — Grok needs no custom manifest (P1)
Users launch Grok and see working, blocked and completed states without installing
an override. Known visible questions take precedence over busy progress.

**Independent Test**: Run a fake Grok executable in an isolated server and
inspect lifecycle, attention and explain results.

**Acceptance Scenarios**:
1. Given Grok busy progress, when an option dialog appears, then blocked wins.
2. Given Grok without progress, when its title or footer establishes activity,
   then the matching rule classifies it; a splash spinner alone is not busy.
3. Given a hooked process, when progress or screen evidence disagrees,
   then semantic hooks retain authority.
4. Given a generic program other than Grok, then Grok rules never apply to it.
5. Given a new process in the same pane, then it never inherits previous progress.

### Edge Cases
- BEL and ST termination, arbitrary byte boundaries and several events per chunk.
- Unknown progress modes, extra fields, nonnumeric or excessive progress values.
- Stale idle titles during visibly active work; background agents at the idle prompt.
- User overrides, process replacement, close, exit and server restart.

## Requirements

### Functional Requirements
- **FR-001**: Retain the latest valid OSC 9;4 progress evidence per live PTY.
- **FR-002**: Reserve progress messages from notification handling, including
  malformed progress, while preserving ordinary OSC 9 notifications and bare BEL.
- **FR-003**: Allow declarative rules to match progress separately from title/screen;
  absent progress must not match even an empty regex. Explain must report the same winner.
- **FR-004**: Bundle Grok rules for `grok` and `grok-build`, with blocked UI above
  working evidence, visible work above stale idle titles, and known-agent idle fallback.
- **FR-005**: Hooks remain authoritative; unknown programs stay plain terminals.
- **FR-006**: Progress is ephemeral, cleared on process replacement/exit/close,
  and absent after a cold server restart; it causes no persisted progress field or per-update events.
- **FR-007**: Offer Grok through the existing bundled-program launcher discovery.
- **FR-008**: Prove the behavior using test-first scanner/rule tests and real PTY IPC tests.

### Key Entities
- **Progress evidence**: Latest terminal progress signal belonging to one PTY.
- **Grok rule set**: Program-scoped declarative state evidence with ordered priorities.

## Success Criteria
- **SC-001**: All scanner boundary fixtures distinguish progress from notifications and bells.
- **SC-002**: Isolated managed Grok reaches working, blocked and completed states within
  five seconds of stable corresponding evidence.
- **SC-003**: Plain-terminal and semantic-hook fixtures show zero spurious notifications
  or lifecycle changes caused by progress.
- **SC-004**: Full repository verification passes before PR publication.

## Assumptions
- Existing rules, lifecycle transitions and hook precedence are reused.
- Grok remains a generic agent; this change supplies detection, not native hooks or resume integration.
- Port current herdr Grok evidence at commit `2563803`; no additional agents in this PR.
- Live authenticated provider checks are follow-up; synthetic PTY proof is reported explicitly.

## Clarification
Requirements reviewed: no unresolved markers. Scope and existing test seams are
already authorized by the user and repository constitution; no extra interview required.
