# Feature Specification: Remember automatic agent permissions

**Feature Branch**: `agents/automatic-permissions`
**Created**: 2026-10-14
**Status**: Ready for implementation
**Input**: Stop repeated Allow requests by fixing managed agent launches upstream, including on another computer after updating Signaltty.

## User Scenarios & Testing

### User Story 1 - Choose automatic execution once (Priority: P1)

A user enables Run agents without confirmations in Preferences. New workspaces started with an installed Codex, Claude or Antigravity command use the provider's automatic execution mode. The choice survives application restart.

**Why this priority**: Repeated tool permission prompts interrupt authorized work.
**Independent Test**: Enable the preference, reopen its saved settings, create a supported agent workspace and inspect its launched command.

**Acceptance Scenarios**:
1. **Given** a fresh or old installation, **When** creating a workspace, **Then** the provider's existing permission defaults apply until automatic execution is selected.
2. **Given** automatic execution is enabled, **When** starting Codex, Claude or Antigravity from the installed command list, **Then** it starts with its native no-confirmation mode. Antigravity starts in execution mode rather than planning mode.
3. **Given** the saved setting is enabled, **When** restarting the application, **Then** it remains enabled.
4. **Given** automatic execution is enabled, **When** choosing a shell, unsupported agent or custom command, **Then** that command remains unchanged.

### User Story 2 - Preserve the choice across orchestration (Priority: P1)

A resumed automatic session and its same-provider worker should not revert to repeatedly asking for permissions.

**Why this priority**: Restart and delegation must not silently undo the selected mode.
**Independent Test**: Report a native session, resume it and start a supported native worker through the existing interfaces; inspect the resulting process arguments.

**Acceptance Scenarios**:
1. **Given** an automatic direct launch, **When** recording and running the provider's standard resume command, **Then** the selected executable and automatic mode survive.
2. **Given** an automatic parent, **When** starting a supported Codex/Claude same-provider task by agent name without an explicit command, **Then** the worker inherits automatic mode.
3. **Given** an automatic parent, **When** starting a different provider or supplying explicit worker arguments, **Then** no mode is inferred or appended.
4. **Given** an explicitly configured resume command, **When** resuming, **Then** its options retain precedence.

### Edge Cases

- Empty commands, unknown programs, shell-promoted agents and mismatched executable names do not receive automatic flags.
- The setting changes future launches only. Already running processes and pending decisions retain their provider policy.
- Unchecking the preference changes new GUI launches; an existing automatic session retains its recorded launch choice on resume.
- Provider-imposed managed policy, authentication and hook trust may still require provider action. No pending request is automatically answered.

## Requirements

### Functional Requirements

- **FR-001**: Offer one persisted, default-off automatic execution preference with text explaining that supported agents may run commands and edit files without asking.
- **FR-002**: Apply it only to supported installed-command GUI launches, preserving custom commands and unsupported programs.
- **FR-003**: Native provider automatic flags must be retained in recorded standard resume commands, without duplication or overriding explicit resume options.
- **FR-004**: Default Codex/Claude same-provider task workers inherit the parent's recorded automatic launch mode; explicit commands and different providers remain unchanged.
- **FR-005**: The preference must read old, missing or mistyped values as disabled, preserve other GUI settings and survive save/load.
- **FR-006**: Reuse native provider execution controls. Do not alter provider-wide configuration, trust, authentication, hook answers or live process modes.
- **FR-007**: Verification must cover persisted choice, launched/resumed worker arguments and native accessible light/dark controls.

### Key Entities

- **Automatic execution preference**: A remembered GUI launch choice, disabled unless explicitly selected.
- **Recorded launch command**: Existing pane arguments containing its chosen native automatic mode.
- **Resume command**: A provider's standard session restoration command; explicit options remain authoritative.

## Success Criteria

### Measurable Outcomes

- **SC-001**: One preference change suffices for subsequent supported GUI launches, including after restart.
- **SC-002**: All tested standard resume and same-provider worker paths preserve automatic mode without adding duplicate flags.
- **SC-003**: All negative controls preserve shell, custom, explicit-worker and cross-provider commands exactly.
- **SC-004**: The shared full verification gate passes, and native controls are readable in light and dark appearances.

## Assumptions

- The user authorizes implementing and publishing a repository fix, not changing configuration on this PC or accessing the other PC.
- Native CLI permissions flags are verified against installed help. Paid provider sessions are outside automated verification.
- Inheritance is limited to Signaltty's generated automatic launch prefix. Arbitrary provider options, permission changes made inside a TUI and wrapper commands are outside this change. Antigravity has no native default task adapter; its workers continue to require explicit commands.
- This is an opt-in product control. The user's current authorization does not change permission defaults for other users.

## Clarifications

### Session 2026-10-14

- No new questions needed: repository defaults and explicit-command precedence settle remaining design choices. Scope, state, interaction, dependencies, failure behavior and proof limits are defined above.
