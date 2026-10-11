# Feature Specification: harness access to Signaltty

**Feature Branch**: `agent/harness-socket-access`
**Created**: 2026-10-11
**Status**: Clarified
**Input**: Fix agent commands unable to reach Signaltty's local socket; inspect other harnesses and open a PR.

## User Scenarios & Testing

### User Story 1 — Control the workspace from a sandboxed agent (P1)

An agent whose shell cannot connect to local sockets can still inspect panes,
start a worker and wait for its output through its harness's integration tools.

**Independent Test**: a real isolated server accepts inspection and pane creation
through the integration channel while a sandboxed socket probe fails.

**Acceptance Scenarios**:
1. Given restricted shell networking, when the agent requests workspace status through the integration, then the result comes from the correct Signaltty server without changing shell network permissions.
2. Given an active pane context, when the agent starts a worker, then Signaltty owns the new PTY and normal state/events apply.
3. Given an unavailable server, when a tool is called, then an actionable error is returned without silently switching servers.

### User Story 2 — Configure supported harnesses without losing settings (P2)

Managed launches and explicit integration installation configure the channel for
Codex, Claude, Cursor and OpenCode. Existing harness settings and approval choices
remain authoritative. Other bundled programs have an evidence-based support guide.

**Independent Test**: isolated configuration fixtures verify all four formats,
repeat installation, disabled/foreign entries, cleanup and provider-root overrides.

**Acceptance Scenarios**:
1. Given unrelated settings and integrations, when installing or removing Signaltty, then unrelated entries remain intact.
2. Given a user-disabled Signaltty entry or conflicting foreign entry, when preparing a launch, then no entry is silently enabled or replaced and the result explains the conflict.
3. Given direct command access denied by the OS, when the CLI reports the failure, then it points to the integration channel rather than suggesting file permission changes.

### Edge Cases

Malformed configuration; nonregular files; concurrent installation; paths containing
spaces/quotes; no pane context; missing server; streaming APIs; malformed/oversized
messages; harness trust requirements; externally containerized harnesses.

## Requirements

- **FR-001**: Provide a harness integration channel for existing request/response workspace, pane and orchestration APIs without relaxing shell filesystem/network restrictions.
- **FR-002**: Bind each bridge to an explicit Signaltty socket and pane context supplied by the hosting session; never fall back to another server.
- **FR-003**: Support initialization, tool discovery, tool execution and structured errors, with bounded input and request execution. Streaming subscriptions/terminal attachment, native approval answers and provider lifecycle/session-report APIs are excluded.
- **FR-004**: Automatically prepare bridge configuration alongside existing integrations for Codex, Claude, Cursor and OpenCode; honor provider roots and existing launch opt-outs.
- **FR-005**: Preserve unrelated configuration, disabled choices, hook trust, approvals and foreign entries; uninstall only owned bridge entries; serialize atomic updates.
- **FR-006**: Surface sandbox-denied CLI connections with useful integration guidance, without claiming every permission error is a sandbox.
- **FR-007**: Record inspected sandbox/integration behavior for every bundled harness, distinguishing verified paths from unsupported or manual setup.
- **FR-008**: Prove the correction with isolated real IPC/process tests and a native Codex sandbox probe, without authenticated provider turns.

### Key Entities

- Bridge: harness-owned process with a fixed session context, exposing Signaltty API tools.
- Managed registration: provider-specific configuration entry owned by Signaltty.

## Success Criteria

- **SC-001**: Workspace inspection and worker launch succeed via the integration while the same session's direct socket probe is denied.
- **SC-002**: All four supported configuration formats pass preservation, idempotence and removal tests.
- **SC-003**: Neither installation nor launch inserts full-access, network-enabling or approval-bypass settings.
- **SC-004**: Full repository verification passes before PR publication.

## Assumptions and scope

Harness-launched local integration processes can reach the host server; a sandbox
around the entire harness/container still requires host integration deployment.
Provider approval/trust prompts remain in force. Starting a worker through Signaltty
is an explicit orchestration operation, not an arbitrary replacement shell tool.
The bridge fixes Signaltty access and can start agy in a Signaltty-owned pane; it
does not grant a sandboxed shell access to arbitrary third-party sockets.

## Clarifications

2026-10-11: Existing authorization covers the correction and PR. No critical product
ambiguities remain. Preserve sandbox boundaries and use the harness integration
channel instead of changing network defaults.
