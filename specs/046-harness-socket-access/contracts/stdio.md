# Stdio bridge

`signaltty mcp serve`: UTF-8 newline-delimited JSON-RPC 2.0, stdout protocol only.
Supports MCP 2024-11-05, 2025-03-26 and 2025-06-18 (fallback latest supported).
initialize → capabilities.tools; notifications/initialized → no reply; ping → {}.
Tools: signaltty_context (session pane plus schema), signaltty_call
({method:string,params:object}) for existing non-streaming IPC. Unknown/malformed
requests use JSON-RPC errors; failed tool calls return content with isError=true.
No subscriber/attach transport, no automatic mutation retry.

Context requires nonempty absolute SIGNALTTY_SOCKET and nonempty SIGNALTTY_PANE.
Tools connect only to this socket and validate pane.get before executing calls.
The bridge grants existing workspace/PTY orchestration capability, including worker
launch; it does not execute arbitrary shell commands in place of the harness shell.
Input at most 1 MiB and tool execution at most 60 seconds.

Provider lifecycle hook/session-report APIs are also excluded from MCP; real harness hooks remain the source of agent identity and lifecycle evidence.
