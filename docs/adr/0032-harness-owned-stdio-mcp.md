# ADR-0032: Harness-owned stdio MCP for workspace access

Status: accepted. Date: 2026-10-11.

## Context

A Codex workspace-write shell on Linux denies Unix socket connect with EPERM
when networking is restricted. The pane environment is correct; socket chmod
cannot fix seccomp. Current Codex native path grants remain denied on Linux.
Enabling all networking or all Unix sockets would grant more than workspace access.

## Decision

Expose existing request/response IPC as two tools in `signaltty mcp serve`,
launched by the harness through stdio outside its shell sandbox. The bridge
requires explicit SIGNALTTY_SOCKET and SIGNALTTY_PANE, checks the hosting pane,
and never falls back to the default socket. It does not impersonate the agent
or inject provider approval decisions. Tool approvals remain the provider's.

Register the bridge alongside managed hooks for Codex, Claude, Cursor and
OpenCode. Serialize atomic configuration updates, preserve foreign entries and
user disables. The exact managed argument vector includes an ownership marker;
it grants no capabilities and supports moving or renaming the executable.
Codex explicitly forwards only the two pane context variables plus any existing
user-selected forwarding. No session values are stored in global configuration.

## Consequences

Agents can inspect workspaces, control PTYs and start workers through existing
server operations while shell networking stays restricted. This deliberately
exposes workspace operations with the socket client's existing authority; the
mutating tool advertises destructive and non-idempotent hints. The read-only
context tool does not grant access to arbitrary other host sockets.

Subscriptions, terminal attach/detach and server shutdown are excluded. Input is
bounded to 1 MiB and tool calls to 60 seconds. A timeout may follow an applied
mutation: inspect state before retrying. Other harnesses can configure the same
stdio command manually where their process boundary permits it. A sandbox
around the entire harness also encloses local MCP children and needs its own
host deployment; this bridge does not defeat that sandbox.

## Evidence

See [feature research](../../specs/047-harness-socket-access/research.md), the
real CLI/server `mcp_bridge` integration tests, and `qa-harness-socket.py`.
The native QA does not perform an authenticated model turn.

Native approval answers and provider lifecycle hook/session-report APIs are also excluded from MCP; real harness hooks remain the source of agent identity and lifecycle evidence.
