# Research

Decision: select native automatic execution at managed launch, not by answering PermissionRequest hooks automatically.
Rationale: the provider must know its policy before making tool requests. An Always hook answer is not uniformly supported and could grant unrelated later requests.
Alternatives considered: changing user provider configuration would affect unrelated terminals; prompt instructions cannot change runtime permissions; copying all initial arguments to resume could replay prompts, planning mode or obsolete options.

Installed CLI help, 2026-10-14: `codex --help` supports `--dangerously-bypass-approvals-and-sandbox`; `claude --help` supports `--dangerously-skip-permissions`; `agy --help` supports `--dangerously-skip-permissions` and `--mode accept-edits`. The last flag explicitly exits a persisted planning default on new launches. These are execution permissions, not hook trust or account authentication.

Decision: use a provider-specific automatic argument prefix generated for bare launch commands. Inherit only that prefix into same-provider bare default worker commands and minimal standard resume commands. Explicit argv remains authoritative. This avoids interpreting option-looking prompt text or provider-specific arbitrary argument grammars. Source and target must refer to the same native program.

Decision: store the launch preference in existing `gui.json`; store session choice in existing Pane.argv. No new IPC field or snapshot schema is needed. Tasks use the existing explicit parent_pane_id reference. Cross-provider workers require explicit argv to choose their mode.
