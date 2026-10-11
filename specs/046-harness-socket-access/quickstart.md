# Quickstart

Run scripts/setup-agent.sh all and scripts/verify.sh doctor. Build the workspace.
Run cargo test -p signaltty-integration --test configuration mcp and
cargo test -p signaltty-server --test mcp_bridge. Run the native Codex sandbox probe
with python3 scripts/qa-harness-socket.py; it owns its listener/server/context and
executes no paid turns. Finish with scripts/verify.sh full and inspect renders.
In a managed Codex/Claude/Cursor/OpenCode pane, approve Signaltty MCP using the
provider's normal workflow and use signaltty_context/signaltty_call when shell
socket access is restricted. Existing sessions need restart to discover new config.
