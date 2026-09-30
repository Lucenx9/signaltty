# ADR-0015: Native permission verdicts and explicit checkout lifecycle

**Status**: Accepted, 2026-09-30. **Spec**: [013-local-agent-workflows](../../specs/013-local-agent-workflows/spec.md).

## Context

Static provider-wide answerability cannot prove a real permission receiver is alive. Numbered terminal answers were byte-fixture verified, while current Claude and Codex offer native PermissionRequest hook verdicts. Worktrees need explicit lifecycle without making workspace closure delete files. GUI zoom must not rewrite the saved layout.

## Decision

- Extend existing hook-event with an optional bounded native wait. A transient server-owned broker registers the response route before publishing a decision. A dedicated reporter connection owns the waiting request. Allow once/Deny use documented native JSON stdout; no automatic or permanent policy grants.
- Answers consume exactly one current live route. Disconnect, deadline, supersession, pane/session loss or shutdown cancels and clears only that route's current decision. Replay/audit never delivers verdicts. Persisted structure cannot restore native answerability. Unsupported channels stay read-only.
- Current Claude and Codex0.159.1 share the narrow allow/deny contract. Codex rejects session permission updates. Provider timeout125s/internal120s replaces the old3s timeout only for interactive permission waiting; status hooks remain short. Native trust remains provider-owned.
- Git is authoritative for registered worktrees. Create/open associate ordinary workspaces with canonical checkouts. Closing a workspace preserves checkout and branch. Removal is explicit, non-force, and refuses main/dirty/locked/live-referenced paths. Removal reservations protect concurrent ordinary workspace creation and pane launches as well as other worktree operations.
- Keep a checkout when convenience workspace insertion fails after successful Git creation; report its path for later open rather than deleting work another process may have started using.
- Palette invokes existing actions. Terminal search and zoom are client view state. Zoom retains hidden VTE widgets, uses the latest server layout when restored, and suppresses divider persistence while projected.

## Consequences

Approval transport is separated from render data and lifecycle observation. Small feature modules hide response lifetime and checkout safety, while existing Store events, GTK action registry and keyed terminals remain in use. The first increment deliberately excludes browser/SSH, permanent permission policy, provider proliferation and per-turn diff claims.

Sources: [Claude hooks](https://code.claude.com/docs/en/hooks#permissionrequest-decision-control), [Codex tagged permission source](https://github.com/openai/codex/blob/rust-v0.159.1/codex-rs/hooks/src/events/permission_request.rs), [design synthesis](../../specs/013-local-agent-workflows/research.md).

Working-tree counts use NUL-separated Git output with rename detection disabled. Renamed files appear as deletion/addition, preserving literal paths including tabs and newlines. Unborn repositories use the empty tree as their comparison base.
