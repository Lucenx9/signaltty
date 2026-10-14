# ADR-0036: Explicit automatic agent launch mode

Status: proposed, 2026-10-14.

## Context

Repeated tool approval prompts interrupt work the user intends to run autonomously.
A prompt telling the agent not to ask does not configure its native permission
policy. Our launcher starts bare provider commands, and resume previously retained
only the selected executable, dropping an explicit automatic launch choice.

## Decision

Add one default-off persisted GUI preference, Run agents without confirmations.
It converts supported installed-command GUI launches into native automatic modes:
Codex's approval/sandbox bypass, Claude's permission bypass and Antigravity's
permission bypass with accept-edits mode. The native switch explains the access
being selected. Provider configuration and live processes are unchanged.

Record the choice in existing Pane.argv. Retain our generated native prefix only
in matching minimal standard resume commands and default same-provider workers
started with agent plus parent_pane_id. Explicit resume options and explicit
worker argv remain authoritative. Do not parse arbitrary command grammars or copy
models, prompts, profiles, planning flags or unrelated initial arguments.

## Consequences

One preference change covers future supported GUI launches and their standard
resume/default same-provider orchestration. Custom commands remain literal.
The switch does not answer pending PermissionRequest hooks or change authentication,
hook trust or managed provider rules. Once/Deny continues to serve native guarded
sessions. Antigravity resume still requires its existing self-reported session
command; this does not add native Antigravity session discovery.

Automatic mode disables native tool approval checks, and Codex also runs without
its native sandbox. It is selected explicitly rather than imposed globally.
Disabling the preference affects future GUI launches; existing session argv retains
the mode chosen at launch. Arbitrary manual options and in-TUI policy changes are
not inferred. Proof uses real PTYs with fixture executables and native GUI controls;
paid provider sessions and another PC's installed build are not certified here.

See [spec 059](../../specs/059-automatic-agent-permissions/spec.md).
