# Research

## Reproduction

Codex CLI 0.162.1 `codex sandbox -c sandbox_mode="workspace-write" -- python3 …`
connecting to an isolated AF_UNIX listener returns EPERM. Repeating with
sandbox_workspace_write.network_access=true succeeds. TMPDIR=/tmp was used to
avoid an unrelated synthetic bubblewrap mount error under /tmp/user/1000.
No provider authentication or turn was used.

## Decision

Use harness-owned stdio MCP rather than enabling all networking or all sockets.
Codex Linux seccomp cannot enforce per-path AF_UNIX exceptions; upstream tests
explicitly show path grants remain denied. Claude shell MCP processes also run
outside its Bash sandbox. Cursor CLI supports stdio MCP with provider approval.
OpenCode supports local MCP registrations and its own tool permission policy.
Externally containerized whole harnesses still need host deployment.

Alternatives rejected: unrestricted network (too broad); all Unix sockets (too
broad); filesystem mailbox transport (custom polling/lifecycle protocol); per-path
socket grants (not usable with current Linux seccomp).

Sources:
- https://github.com/openai/codex/blob/main/codex-rs/linux-sandbox/README.md
- https://github.com/openai/codex/blob/main/codex-rs/linux-sandbox/tests/suite/managed_proxy_unix_sockets_tests.rs
- https://code.claude.com/docs/en/sandboxing
- https://code.claude.com/docs/en/mcp
- https://cursor.com/docs/cli/mcp
- https://opencode.ai/docs/mcp-servers/
- https://geminicli.com/docs/cli/sandbox/
- https://modelcontextprotocol.io/specification/2025-06-18/basic/transports

An isolated native Claude `mcp add-json --scope user` probe confirmed that
CLAUDE_CONFIG_DIR places global MCP registrations in <override>/.claude.json.
