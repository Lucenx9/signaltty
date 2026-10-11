# ADR-0031: First-prompt workspace titles

Date: 2026-10-11. Status: accepted.

## Context
T3 Code uses conversation-derived titles. SignalTTY owns terminal sessions, not
an agent chat transcript, but Codex and Claude expose UserPromptSubmit hooks.
Manual workspace names and immutable handles already exist.

## Decision
Opt-in workspace.create auto_name starts in awaiting_prompt. The GUI defaults to
this for supported installed Codex/Claude commands; manual/custom/other commands
retain explicit naming. Only the first accepted non-empty prompt hook starts a
background metadata invocation. Do not scrape terminal input or persist the prompt.
Use each CLI's server-environment default account/model; do not fall back to a
different provider/account. No new API key is required. Account quota can be used.

Persist typed naming states: manual, awaiting_prompt, generating, generated,
failed. Manual rename sets manual and makes an in-flight result inapplicable,
even if it repeats the provisional label. Handles never change. On restart,
generating becomes failed; no replay or automatic retry.

Generation has two slots, a 60-second deadline including queueing, bounded input
and output, private temporary working directories, argv/stdin separation and
process-group cleanup. Claude disables tools, skills, hooks and MCP. Codex uses a
read-only sandbox, disables shell/code execution features, apps/plugins/hooks and
other metadata-unrelated features, and explicitly disables every named MCP server
from local CLI metadata. Empty MCP tables merge rather than remove user config.
Unsupported/unavailable providers, errors and invalid results keep the provisional
name and expose failure in workspace metadata. Existing/CLI workspaces default
manual unless explicitly opted in.

## Consequences
The title updates on all clients and does not depend on a live GUI. First-prompt
hooks are required; screen-only agents do not get invented titles. Test fixtures
prove state, race, IPC and subprocess behavior without claiming paid-provider
model quality or account compatibility. Native title rows use full width and keep
status on the activity line, so longer AI titles have room without hiding urgency.
