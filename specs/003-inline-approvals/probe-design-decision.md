# Probe design decision: synchronous approval hooks

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

Date: 2026-10-02

Status: Proposed, response channel empirically verified; implementation pending.

## Context

The GUI currently reports permission attention but cannot answer requests.
Claude `PermissionRequest` hook verdicts accepted and denied a harmless Bash
command in live interactive signaltty panes. The same probe demonstrated
terminal fallback and rejection of a late verdict after hook timeout.
See [the results](probe-results.md).

Claude can resolve a prompt in its TUI while its permission hook remains alive.
Focus currently clears attention in signaltty. Neither hook lifetime nor
attention alone represents an unresolved decision.

## Decision

Use a held Unix-socket hook connection and one server-owned pending decision
per pane. The adapter interprets requests and emits verified verdicts. Start
with Once and Deny for Claude Bash permissions. Other capabilities remain
read-only until live probes pass.

Register the responder and decision atomically through Store. The connection
task watches reply, EOF, and deadline together. Do not block EOF detection by
awaiting a pending hook inside the ordinary router dispatch branch.

Decision lifetime is separate from focus and attention acknowledgement.
Actual terminal input yields the decision to the TUI before PTY forwarding.
Semantic completion, transport loss, expiry, and pane/session exit also retire
it. Restore never resurrects an actionable hook channel.

Short polling requests were considered. They add a method, owner token,
result retention, and leases or process monitoring. The held connection
provides simpler lifetime ownership using the existing socket transport.

## Consequences

The GUI remains a client and the server keeps owning PTYs. Inline updates must
preserve VTE widgets. The connection loop needs a pending-response path.
Consumed IDs need bounded retry history; state transitions retain paired events.

Server acceptance means a choice was queued once, not that Claude acknowledged
execution. Empty hook stdout yields normal terminal handling after cancellation
or timeout. The implementation must test terminal input races, late answers,
duplicate clients, supersession, and restart before exposing clickable answers.

Always policy, AskUserQuestion, ExitPlanMode, and other adapters require their
own verified payload and response semantics. None may fall back to guessed keys.
