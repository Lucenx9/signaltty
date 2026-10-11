# Workspace panel and automatic AI titles

Created 2026-10-11. Branch t3/workspace-ai-titles. Follows merged PR #91; submitted as a separate follow-up PR.

## User intent
Polish the workspace panel and name workspaces automatically like T3 Code.
Clarification: the user explicitly chose AI from the first message.

## Stories and acceptance
1. Read longer workspace titles: title occupies its own row with project mark and
close control; lifecycle/attention remain visible on the next row. Keep section
ordering, selection, tooltips, child collapse, keyboard navigation and stable PTYs.
2. Create without inventing a name: default automatic naming in the native dialog,
provisional folder name, optional manual naming. First accepted UserPromptSubmit
hook provides the first prompt. Do not scrape terminal input.
3. Server owns generation: background bounded Codex/Claude metadata invocation,
using installed CLI authentication and no new API key. Hook reply stays immediate.
Only one first-prompt attempt per opted-in workspace; legacy/CLI workspaces remain
manual unless auto_name requested. Successful title updates existing workspace;
immutable handle, directory and sessions remain intact.
4. Respect user intent: manual rename disables generation including in-flight
results. Invalid output, timeout, unavailable provider and shutdown preserve the
provisional name and expose failure; no automatic retry on later prompts. Persist
naming state; interrupted generation becomes failed after restart.

## Constraints
First-prompt title generation currently supports Codex and Claude native hooks.
Other agents keep their provisional name and expose unsupported generation when
an explicit prompt hook arrives. Use a private temporary working directory,
stdin prompt data, bounded input/output, timeout and process-group cleanup.
Claude metadata invocation disables tools/skills/MCP/hooks. Codex uses read-only
sandbox and disables shell features; no approval bypass. Do not claim live paid
provider proof from fake generator tests. Generation uses the CLI's configured
model/account, and may consume account quota. Document this in the dialog/docs.

## Verification
Test-first Store lifecycle/race rules, fake-process generation behavior and real
server IPC hooks. Native light/dark panel and dialog snapshots, narrow and large
text. Full verification and independent visual review required.
