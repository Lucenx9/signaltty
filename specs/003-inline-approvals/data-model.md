# Inline approval data model

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

This is the proposed model for spec 003. These fields are not implemented yet.

## Decision

`Pane.decision: Option<Decision>` projects one current request to clients.
A Decision has an opaque server-generated `id`, a sanitized `prompt`, ordered
`options`, `received_at`, and an `answerable` capability flag. Each option has
an opaque `id` and a plain-text `label`. Neither field contains executable text.

Only an established, verified response channel makes a decision actionable.
The first capability is Claude Bash permission Once/Deny. A prose-only hook
produces no decision options. Unsupported structured requests are read-only.

The decision ID is distinct from the IPC request ID and Claude session ID.
Claude's observed `PermissionRequest` input had no `tool_use_id`.

## Pending hook

Server runtime state holds the pane ID, decision ID, one-shot responder,
deadline, and semantic request identity. The connection task owns the matching
receiver and cleanup lease. These types never enter core or snapshot storage.

Normalize and bound tool input at the adapter boundary. Keep the original
tool name and input identity for matching semantic completion. Tool input
is untrusted data, never a command for the server to execute.

Store transitions atomically register the responder and projection, consume
a valid choice, or cancel the matching decision. Each visible mutation emits
`pane.updated`. Do not hold the Store lock during socket I/O or waits.

## Lifetime

Registration replaces any older decision and releases its hook without a
verdict. A valid answer consumes the current responder once. Bounded recent
consumed IDs distinguish retries from unknown stale requests.

Cancellation includes expiry, transport loss, supersession, pane close/exit,
session termination, semantic resolution, and actual user terminal input.
Input invalidation occurs before the PTY write and yields control to the TUI.
Looking at a pane, attaching without input, or resizing does not resolve it.

An old cleanup lease matches both pane and decision ID. It cannot remove a
newer request. Failed responder delivery retires actionability without a PTY
fallback. Queueing a reply establishes server acceptance, not agent execution.

Snapshots omit pending decision state. Restore cannot re-create an actionable
request without a live hook channel. No policy or Always behavior is persisted.

Validation must cover live pane ownership, adapter/tool capability, ID and
option membership, sanitized prompt bounds, and existing hook payload limits.
