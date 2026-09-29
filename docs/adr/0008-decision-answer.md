# ADR-0008: Structured Decisions Answered In-Band

**Status**: Accepted (2026-09-29) · **Spec**: `specs/003-inline-approvals/`

## Context

Directive 2 (docs/14 §2) requires inline approvals: answer where the user
looks, never inside the agent's TUI. Decisions must travel as data
(prompt + options), and delivery must not be guessed per agent.

## Decision

1. **One pending `Decision` per pane**, set only by `hook-event`
   (`decision: {id, prompt, options[{id, label}]}`), stored on `Pane`,
   superseded by newer (single `decision.created` carrying `prev`).
   Prose-only events neither set nor clear it.
2. **One answering method**: `decision.answer
   {pane_id, decision_id, option_id}`. It consumes the id *before*
   delivering, so a concurrent clear can never double-deliver. Stale or
   consumed ids are typed `NO_SUCH_DECISION` (client refreshes, drops its
   bar); unknown options and channelless adapters are `BAD_PARAMS`.
3. **One channel per adapter** (`AgentAdapter::answer_channel`, default
   `None` = read-only). First probe: Codex `TypeText` (1-based option
   number + Enter over the `pane.input` write path — same bytes, same
   rate limits; the channel, not a fallback). Others render prompt + hint
   until probed.
4. **`answerable` captured at ingest** and stored on the `Decision`, so
   the GTK GUI (which must not depend on `signaltty-agent` per ADR-0004)
   and the CLI render buttons vs. hint from data alone.
5. **Clearing**: an agent transition clearing required attention,
   lifecycle leaving `blocked` (`moved_on`), child exit (`pane_exited`),
   or answer (`answered`). Reading and focusing preserve unanswered
   decisions and their gates, including `pane.mark_seen` and default
   attach. ADR-0012 corrects the original acknowledgment semantics.
   "Always" answers once — no policy storage in v1.

## Consequences

- GUI/CLI never scrape terminal text for decisions and never rebuild the
  terminal when the bar toggles (persistent widget, id-keyed reconcile).
- Live-Codex end-to-end (click → agent resumes with that choice) is
  fixture-verified at the byte level; confirmation against the real TUI
  is tracked as follow-up work in the spec, not a merge blocker.
- Future channels (`HookVerdict`, `StdinJson`, …) extend `AnswerChannel`
  without changing the method or the events.
