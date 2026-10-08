# Feature Specification: Harness event normalization

**Feature Branch**: `t3/harness-integration-fixes`
**Created**: 2026-10-08
**Status**: Implemented and verified
**Input**: Audit harness integration against cmux/herdr attention semantics and T3 Code presentation quality, with independent native xAI Grok 4.7 review.

## User Scenarios & Testing

### User Story 1 — Failed runs need attention (P1)

An OpenCode run that fails must appear as failed and request attention, with its error message, rather than appear successfully completed.

**Independent Test**: Execute the installed plugin through both supported entrypoints with provider-shaped events and inspect reporter arguments/payloads; deliver normalized error events through the existing server IPC seam.

**Acceptance Scenarios**:
1. Given a working V2 session, when execution fails, then Signaltty receives a session error containing its identity and provider message, and shows Failed/Error.
2. Given a V1 session error with a nested provider error, then the message survives normalization.
3. Successful completion still reports idle; interruption retains the existing idle behavior.

### User Story 2 — Identity survives provider event shapes (P2)

The session is available for explicit resume as soon as the provider reports its creation.

**Independent Test**: Execute a V1 session creation event containing its identity in session information and assert the reporter receives that identity.

**Acceptance Scenarios**:
1. Creation events with `properties.info.id` report the session identity.
2. Existing top-level session identity forms remain accepted.
3. Outside a managed pane, the plugin invokes no reporter. Unload cancels the V2 subscription.

### User Story 3 — Answered approvals resume work (P1)

After an answer to a pending approval, a blocked pane returns to Working.
A stale answer or a decision cleared without an answer must not resume work.

**Independent Test**: Store transition tests and real native permission-reporter
IPC tests for Claude/Codex. Existing TypeText answers use the same transition.

**Acceptance Scenarios**:
1. An accepted answer clears the decision and returns Blocked to Working.
2. Superseded/stale answers do not change lifecycle.
3. Timeout, cancellation and disconnect keep existing lifecycle behavior.

### Edge Cases

Errors without a message use the existing unknown fallback; invalid V2 event envelopes and unrelated events do not break the provider. No user configuration or real provider sessions are used by tests.

## Requirements

- FR-001: Normalize V2 execution failure as a session error, preserving identity and message.
- FR-002: Read V1 creation identity from session information as well as the already supported fields.
- FR-003: Preserve nested V1/V2 provider error messages and legacy direct messages.
- FR-004: Keep both plugin loaders, unmanaged-pane no-op, bounded reporter execution and subscription cleanup functional.
- FR-006: Answering a current blocked decision resumes the pane through Store transitions; stale answers and non-answer clears do not.
- FR-005: Exercise the installed JavaScript plugin behavior in the shared verification gate, without paid model calls.

## Success Criteria

- SC-001: All failure fixtures report errors and all success fixtures retain successful completion semantics.
- SC-002: Creation fixtures report the expected session immediately.
- SC-004: Accepted blocked approval fixtures resume Working; stale/non-answer fixtures remain blocked.
- SC-003: Focused red/green tests and full workspace verification pass.

## Assumptions and scope

This repairs the existing OpenCode integration contract and shared approval lifecycle transition. No provider taxonomy, new permission transport, IPC schema, GUI widgets or persistent schema changes. Clarification review found no unresolved requirements. Apple/Emil principles apply through accurate, immediate state feedback in existing native controls; no decorative motion is required.
