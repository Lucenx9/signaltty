# Proposed inline approval IPC

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

This contract is planned. The methods and fields are not available yet.
Existing framing, peer identity, and error rules remain in
[docs/08-ipc.md](../../../docs/08-ipc.md).

## Register a waiting hook

Extend `hook-event` with optional typed `wait_for_answer: bool`, default false.
Only a proven adapter/tool capability may create an actionable decision.
Resolve pane context through the existing explicit ID or process ancestry.
Register its responder before emitting `pane.updated`.

A true value uses a dedicated connection with one outstanding request.
The response remains pending until a choice or cancellation. The connection
task watches EOF and the bridge deadline while awaiting the responder.
Further client input closes that hook connection and invalidates its request.

An answered result contains `{accepted: true, decision_id, verdict}`. The CLI
prints only the adapter-produced `verdict` JSON to hook stdout.
A yielded result contains `{accepted: true, yielded: true, reason}`; the CLI
prints nothing and exits successfully so Claude keeps terminal handling.
An unavailable server follows the same CLI stdout fallback, with stderr diagnostics.

## Answer a decision

`decision.answer {pane_id, decision_id, option_id}` validates and consumes the
live responder under one Store lock. Supported option IDs are `once` and `deny`
for the initial Claude Bash capability; callers use the returned options list.

The result is `{accepted, already_answered}`. A new choice returns true/false.
A retry for a recently consumed ID returns true/true without another verdict.
Accepted means queued to the hook, not acknowledgement of execution by Claude.

Mistyped params or an invalid option use `BAD_PARAMS`. Missing panes use
`NO_SUCH_PANE`; exited panes use `PANE_EXITED`. Unknown or superseded decisions
use a new proto constant `NO_SUCH_DECISION`. A disconnected responder returns
an unavailable receipt and retires the projection. No raw-input fallback exists.

Add the method and new error to proto canonical lists and the self-printing
schema, the implemented IPC documentation, and integration tests together.

## Projection and events

`pane.get` and `workspace.get` include the optional decision projection described
in [data-model.md](../data-model.md). `pane.updated` accompanies registration,
consumption, and cancellation. Absent decisions retain normal terminal behavior.

Observation-only hook calls retain their current immediate response. Focus and
`pane.mark_seen` never send a hook verdict or consume the decision.
