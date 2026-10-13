# Decisions for accepted report retries

- Decision: Match accepted report content and exclude the server-assigned timestamp. Rationale: a retry receives a new handler timestamp even when the worker resends identical content. Alternative: a new caller key would require new stored fields and does not solve replacing content under a reused key by itself.
- Decision: Keep equality and terminal-state rules in `Task::apply_report`; expose whether a change occurred. Rationale: Store must suppress both report events for retries under the same write lock. Alternative: a handler-only check would leave other Store callers with different semantics.
- Decision: Reuse persisted `TaskResult`. Rationale: snapshot-restored retries can use the existing accepted content. Alternative: a report delivery outbox is a separate coordinator feature and cannot be implied by acknowledgment of this API.

The [Agent Orchestrator research](../../docs/research/2026-10-12-agent-orchestrator.md) motivated an explicit acceptance boundary. This feature improves worker-to-server retry handling only. It does not claim coordinator delivery, crash-proof acknowledgment before snapshot flush, or provider understanding of a report.
