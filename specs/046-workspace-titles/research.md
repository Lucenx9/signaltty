# Workspace title research

Primary sources inspected on 2026-10-11:
- [T3 Code thread sidebar](https://github.com/pingdotgg/t3code/blob/main/docs/user/thread-sidebar.md)
- [T3 Code Codex metadata generation](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/textGeneration/CodexTextGeneration.ts)
- [T3 Code Claude metadata generation](https://github.com/pingdotgg/t3code/blob/main/apps/server/src/textGeneration/ClaudeTextGeneration.ts)
- [Codex configuration source](https://github.com/openai/codex/blob/main/codex-rs/config/src/config_toml.rs)
- Local installed codex exec/claude help and codex MCP metadata commands.

T3 generates concise titles from conversation context using server-owned provider
metadata requests. Claude title requests use isolated directories and disable tools,
MCP, skills and hooks; Codex uses ephemeral read-only structured output. SignalTTY
has no chat transcript, so its native UserPromptSubmit hook is the first-message
boundary. The GUI enables this for supported Codex/Claude choices only.

| Direction | Benefit | Tradeoff | Decision |
| --- | --- | --- | --- |
| Name/status share first line | Short project labels look dense | AI titles truncate early | Previous layout |
| Title line, activity/status below | More title width, preserves urgency and project context | Activity text can ellipsize | Selected |
| Title-only compact rail | Maximum density | Hides attention and useful context | Rejected |

The preliminary title/status geometry test failed on the shared-line layout and
passed once status moved. Duplicate headline/status wording is hidden. Native
large-font dialog renders led to shortening the subtitle to “From your first
message”, with account/model details in a tooltip.

Codex MCP tables merge: an empty override left two configured servers. Explicit
per-name enabled=false entries in a TOML table were verified locally to leave zero enabled
servers. The generation path uses that explicit metadata-only configuration.
No live paid model request was used as verification.

The final large-font worker + Approval scene exposed excessive truncation on the
shared activity row. Optional task chips now occupy a dedicated line below the
headline/status. A geometry regression failed before this change; final native
captures and the full gate verify it. Ordinary workspaces keep their compact rows.
