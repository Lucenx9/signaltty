# Inline approvals: response-channel research

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

**Checked:** 2026-10-02. **Scope:** first-party documentation and source inspection.
**Evidence status:** Claude's synchronous `PermissionRequest` verdict was
verified in live interactive sessions, including two signaltty panes.
See [empirical results](probe-results.md) for versions, outcomes, and limits.
The GUI and server response bridge remain unimplemented. Rolling documentation
may describe capabilities beyond an installed version.

## Documented Claude Code hook channels

`PreToolUse` runs before every tool call; `PermissionRequest` runs when a
permission decision is needed. The latter receives `tool_name`, `tool_input`,
and optional permission suggestions, but no `tool_use_id`. Suggestions are not
an exact list of dialog options. A generated request id is therefore necessary.
[Hook input reference](https://code.claude.com/docs/en/hooks#permissionrequest-input).

The two output shapes differ:

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}
```

```json
{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}
```

For a permission denial, use `decision.behavior: "deny"` and an optional
`message`; exit code 2 alone does not deny a `PermissionRequest`.
`updatedPermissions` can apply session-scoped or persisted changes. An allow
verdict does not override deny policy. `AskUserQuestion` and `ExitPlanMode`
need special input handling; a generic allow is insufficient.
[Decision control](https://code.claude.com/docs/en/hooks#permissionrequest-decision-control),
[PreToolUse control](https://code.claude.com/docs/en/hooks#pretooluse-decision-control).

Command hooks default to 600 seconds in the current reference. Explicit timeout
cancels the hook and discards output. A timed-out `PreToolUse` command hook
continues through normal permissions; SDK callback timeouts differ. Async hooks
cannot supply a synchronous approval verdict.
[Timeout reference](https://code.claude.com/docs/en/hooks#timeouts).

## Claude SDK control protocol

The official SDK exposes `canUseTool`/`can_use_tool`, including permission
requests and `AskUserQuestion`. Its callback can remain pending indefinitely.
This is a documented custom-client interface, not evidence that an already
running terminal session accepts control JSON on its PTY.
[SDK user input](https://code.claude.com/docs/en/agent-sdk/user-input).

The official Python SDK source handles CLI `control_request` messages with
subtype `can_use_tool`, invokes the callback, then writes a matching
`control_response` with `request_id` and allow/deny data. Canceled requests
do not receive a response. Keeping stdin open is essential.
[Control protocol source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/query.py).

Its subprocess transport uses `--input-format stream-json`; the permissions
configuration can also pass `--permission-prompt-tool`. Integrating this path
would require owning a structured subprocess transport rather than sending
JSON as ordinary terminal input.
[Subprocess source](https://github.com/anthropics/claude-agent-sdk-python/blob/main/src/claude_agent_sdk/_internal/transport/subprocess_cli.py).

## Documented Codex app-server channel

App-server issues JSON-RPC requests:
`item/commandExecution/requestApproval` and `item/fileChange/requestApproval`.
They carry thread, turn, and item identifiers. The client responds using the
same request id; decisions include `accept`, `acceptForSession`, `decline`, and
`cancel`. Commands can also offer a policy amendment. Respect
`availableDecisions` when supplied. `serverRequest/resolved` signals an answered
or cleared request, followed by item completion. `item/tool/requestUserInput`
has a separate answer schema.
[Approval protocol](https://developers.openai.com/codex/app-server#approvals).

Current documentation also describes a terminal client connected to a shared
server:

```sh
codex app-server --listen ws://127.0.0.1:4500
codex --remote ws://127.0.0.1:4500
```

Unix socket listeners are documented too. This provides a candidate for keeping
a terminal UI with structured responses; cross-client request routing and
cancellation must be tested. WebSocket support is marked experimental.
CLI-generated schemas match the installed version.
[Terminal connection and transport](https://developers.openai.com/codex/app-server#connect-the-cli-terminal-ui),
[Schema generation](https://developers.openai.com/codex/app-server#message-schema).

## Fit with the existing repository

The Claude installer currently registers lifecycle and notification events,
not `PermissionRequest`, and sets a 10-second timeout. These observation shims
cannot wait for an ordinary human decision unchanged.
[Installer](../../crates/signaltty-cli/src/integration.rs).
`hook-event` currently publishes data and returns acceptance; it has no pending
response transport. The IPC client can wait for a response, so a dedicated
blocking hook request is a candidate, not an implemented capability.
[IPC contract](../../docs/08-ipc.md),
[CLI client](../../crates/signaltty-cli/src/client.rs).

## Recommended probes and implementation gate

The results cover the first two experiments and concurrent pane attribution.
The remaining race and cancellation cases are implementation gates:

1. In an isolated interactive Claude session, force one harmless Bash request
   using `PreToolUse: ask`. Have a synchronous `PermissionRequest` command hook
   publish the request and wait. Return allow JSON after an external selection;
   require a unique output marker from the command to prove execution. Repeat
   with deny and require that marker to be absent. Headless `PreToolUse` success
   alone does not prove interactive permission handling.
2. Let that hook exceed a short configured timeout. Record what the terminal
   displays, whether it accepts an ordinary user answer, and whether a late
   external response is ignored. Also test cancellation and process exit while
   waiting. Never keep a clickable decision after its response transport dies.
3. If Claude passes, test two concurrent panes, supersession, duplicate answers,
   and a direct terminal answer. Confirm whether terminal answering is possible
   while the hook waits; do not assume the prompt is already visible.
4. For Codex, inspect installed command help and generated schemas first. Probe
   an app-server approval before testing a remote terminal plus a second client.
   Check which client receives the request and whether another client can resolve
   it. Do not infer multi-client behavior from the existence of remote mode.

For the smallest change to today's terminal-plus-hooks architecture, probe
Claude `PermissionRequest` first. A successful result could justify one
dedicated hook-verdict transport with Once and Deny initially. “Always” needs
separate proof of its exact session semantics. Keep every other adapter
read-only until its own live probe passes. Plan approval and questions also
need independent probes before sharing that capability.

## Architecture comparison after the probes

A held hook connection can register the decision and its one-shot responder
atomically. The connection task watches the reply, EOF, and deadline together.
This requires changing the connection loop: it currently awaits `dispatch`
inside its read branch, so a waiting handler alone cannot observe EOF.
[Connection loop](../../crates/signaltty-server/src/server.rs).

Short polling calls preserve the current request-response loop. They require
an owner token, result retention, and a lease or process watcher. A lease leaves
a stale-answer window after the hook dies. A process watcher adds Linux-specific
machinery. Prefer the held connection for the first implementation.

Neither design detects a terminal answer from socket lifetime alone. The live
probe showed that Claude can execute the command while its permission hook
still waits. `PostToolUse` arrived with the same `tool_name` and `tool_input`
before the hook returned. The implementation must combine semantic resolution
with transport cleanup and test direct terminal answers during a long command.

Focus also differs from resolution. `pane.mark_seen` and normal attachment
clear attention today. They must not consume a live decision.
[Attention transition](../../crates/signaltty-server/src/store.rs),
[Focus behavior](../../crates/signaltty-gui/src/terminal.rs).
