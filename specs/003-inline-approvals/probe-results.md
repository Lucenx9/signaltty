# Claude approval channel: empirical results

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

The synchronous Claude `PermissionRequest` hook can accept or deny a Bash
permission while Claude keeps its terminal UI. Both outcomes were verified
inside signaltty. This establishes a response channel for the first adapter;
it does not implement the server bridge or the GUI buttons.

## Environment and procedure

The probes ran on Linux on 2026-10-02. The initial Claude version was 2.1.287.
Later terminal headers showed 2.1.288. Both versions appear in the
[structured evidence](probe-evidence.json). Codex 0.160.0 was inspected with
`--help`; no Codex approval channel was tested.

Each live session used Haiku, a temporary working directory, an explicit
settings file, empty normal setting sources, and an empty strict MCP config.
No permission bypass flag was used. Authentication used the existing Claude
login. Sessions can create their own transcript and workspace trust records;
the probe does not install hooks into personal settings.

The requested command was `printf approved > approval-proof.txt`.
A `PreToolUse` hook returned `permissionDecision: "ask"` to force a permission
request for this harmless command. The `PermissionRequest` hook recorded its
input, waited for an external response file, then emitted the Claude verdict.
For an approval, the file had to contain `approved`. For a denial, the file
had to remain absent and the terminal had to report hook denial.

The response file represented the future GUI selection. This was a live CLI
experiment, not a click through the current signaltty GUI.

## Observed outcomes

| Case | Observation |
|---|---|
| Headless `PreToolUse` allow | The command executed and created the marker. |
| Headless `PreToolUse` deny | No marker; Claude reported a permission denial. |
| Interactive `PermissionRequest` allow | The external verdict created the marker. |
| Interactive `PermissionRequest` deny | No marker; the TUI reported hook denial. |
| Direct terminal answer | Enter on the displayed Yes option created the marker while the hook still waited. |
| Hook timeout, 2 seconds | Claude terminated the hook. The terminal permission prompt remained usable. |
| External allow after timeout | No marker during the observation interval. Enter in the terminal then created it. |
| Late deny after terminal approval | The hook emitted the late verdict, but the already approved command remained completed. |
| Semantic completion | `PostToolUse` arrived with the same tool name and input before the waiting permission hook returned. |
| Two simultaneous signaltty panes | Allow executed only in its pane; deny did not execute in its pane. Each hook received its correct `SIGNALTTY_PANE` and isolated `SIGNALTTY_SOCKET`. |

The signaltty cases used a separate server, socket, state directory, and empty
plugin directory. The panes were closed and the server was shut down afterward.
The controller used the existing IPC for spawn, read, and terminal input;
approval itself used hook stdout, not synthetic permission keys.

## Consequences for implementation

Choose Claude hook verdicts as the first channel. Initially offer Once and
Deny for Bash permissions. Keep Always, plan approval, user questions, and
other adapters read-only until their own probes pass.

The hook input did not contain `tool_use_id`. Generate an opaque decision ID
when the server registers the pending request. Do not derive identity from
the displayed question or terminal text.

Use a dedicated held IPC connection for the hook response. Register the
response receiver before publishing an actionable decision. Watch the
connection and deadline concurrently, and invalidate the decision before
yielding to terminal handling. This is the chosen design direction in
[the plan](probe-plan.md).

Transport lifetime alone cannot indicate resolution. A terminal answer can
win while the hook still waits. Use semantic signals as well, and retain the
distinction between a server accepting a choice and Claude applying it.
The probes establish an at-most-once server delivery target, not a protocol
acknowledgement from Claude.

`pane.mark_seen` currently clears attention on focus. Focus must not consume
the future decision. Persist no actionable hook transport across restart.

## Remaining acceptance checks

The implementation still needs duplicate-client and stale-ID arbitration,
supersession, disconnect cleanup, restart cleanup, and a real GUI click.
Direct terminal approval during a long command, direct terminal denial, and
Escape need separate cancellation tests. `PostToolUse` proves completion,
but it does not prove that the hook stops when execution starts.

The timeout case proves rejection of one late verdict during an observation
interval. The concurrent pane case proves attribution, not server-side
`decision.answer` arbitration; that method does not exist yet.

Use [the quickstart](quickstart.md) to repeat the native allow/deny experiment.
