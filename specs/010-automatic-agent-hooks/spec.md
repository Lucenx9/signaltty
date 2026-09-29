# Automatic agent hooks

Status: Completed. Delivered on main as 6cd930b; CI run 36643280057 passed. User approved automatic setup on supported agent launches and delivery to main.

## User scenarios

1. Select Claude/Codex/OpenCode/Cursor when creating a workspace: Signaltty configures its existing integration before the provider starts; semantic events drive Working/Blocked/Done labels.
2. Split with a supported agent or resume an agent: the same preparation runs before execution.
3. Existing hook configuration survives setup, refresh and uninstall. Invalid configuration produces a visible setup notice while the agent/terminal still launches.

## Requirements

- FR-001: Prepare supported direct argv launches at the server PTY boundary, including split and resume. Agent hints alone do not authorize installation. Shells and arbitrary commands do not install other providers; agents typed inside an existing shell use previously installed integrations or the explicit install command.
- FR-002: Share the installer between CLI and server without introducing OS behavior into pure adapters or a CLI-to-server dependency.
- FR-003: Preserve foreign keys/hooks, reject malformed JSON or foreign OpenCode plugin collisions without writing, refresh owned commands, remove only owned hook commands, serialize concurrent writes, use atomic replacement and preserve existing file permissions. Repeated setup is a byte-preserving no-op.
- FR-004: Resolve an executable CLI reporter and encode executable paths safely. Config roots honor supported provider environment overrides; hermetic tests never write real user configuration.
- FR-005: Return additive integration setup reports from spawn/split/resume; GUI shows actionable notices, CLI preserves JSON stdout and reports human notices. Failure must not prevent the process launch.
- FR-006: Never bypass Codex hook trust or enable intentionally disabled hooks; explain native /hooks review when Codex integration is newly configured. Installed means configured, not proven active.
- FR-007: The ambiguous command agent is not a builtin Cursor alias; explicit detection manifests may declare aliases.

## Acceptance

Real PTY fake-provider tests observe configuration before execution and deliver installed hook commands to actual lifecycle state. Preservation, ownership, path encoding, idempotence, concurrency, failure and resume regressions pass. Workspace build/tests/clippy/fmt and native display tests pass; push to main and verify CI.

## Scope

Uses existing semantic hook adapters. No provider API migration, shell parsing, trust bypass, automatic restart of existing agents or global provisioning of all providers.
