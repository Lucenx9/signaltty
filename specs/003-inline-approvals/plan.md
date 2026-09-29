# Implementation Plan: Inline Approvals

**Branch**: `003-inline-approvals` | **Date**: 2026-09-29 | **Spec**: `specs/003-inline-approvals/spec.md`

**Input**: FR-001..FR-005, SC-001..SC-003. Deferred-status lifted: Codex
typed-text is the first probed channel (fixture-verified byte delivery;
live-TUI confirmation is follow-up work, recorded below).

## Summary

One pending `Decision` per pane, set by `hook-event`, answered through a
new `decision.answer` method that delivers via the pane adapter's channel.
GUI renders an inline bar (prompt + buttons) without rebuilding the VTE.
Prose-only attention never renders buttons. "Always" answers once (no
policy storage, per spec Assumptions).

## Technical Context

**Language/Version**: Rust 1.85, edition 2021
**Primary Dependencies**: gtk4/libadwaita/vte4 (GUI only), tokio, serde
**Storage**: in-memory `Store` + existing snapshot (decisions persist like
`last_message`; never secrets)
**Testing**: `cargo test --workspace`; server integration via
`signaltty-testkit`; GUI headless unit tests (display tests only if no seam)
**Target Platform**: Linux
**Performance Goals**: answer path = one `pane.input`-sized write; no new
polling, no layout rebuild on bar toggle
**Constraints**: typed `params.rs` decode; codes from `signaltty-proto::code`;
state changes only through `Store` transition methods (paired emits);
GUI structural reconcile
**Scale/Scope**: 1 method, 3 events, 1 code, 1 model type, 1 trait method
(with default), 1 GUI widget, 2 CLI flags/commands

## Constitution Check

- I (spec-driven): spec exists; this plan + tasks.md complete the flow.
  Spec Assumptions said "design only until probes" — the probe decision
  (Codex typed-text first, fixture-verified) is recorded here instead of
  blocking: live-TUI confirmation becomes explicit follow-up, not a guess
  (no blind fallback: only Codex advertises a channel, others read-only).
- II (product bar): implements directive 2 verbatim (Once/Always/Deny as
  data-driven buttons; ExitPlanMode/AskUserQuestion reuse the mechanism).
- IV (TDD): red-green per slice at agreed seams (see tasks).
- V (boundaries): core holds `Decision`; proto holds names; server owns
  transitions; GUI never scrapes VT for decisions.
- VII (simplicity): no policy store, no per-adapter delivery daemons, no
  new polling. Stale/double answers are ok-results (`answered:false`),
  not new error machinery — except genuinely unknown ids, which reuse
  the `NO_SUCH_*` family via one new code.

## Project Structure

```text
specs/003-inline-approvals/
├── spec.md
├── plan.md              # this file
└── tasks.md             # Phase 2 output

crates/signaltty-core/src/model.rs      # Decision, DecisionOption, Pane::pending_decision
crates/signaltty-proto/src/lib.rs       # decision.answer, decision.{created,answered,cleared}, NO_SUCH_DECISION
crates/signaltty-agent/src/types.rs     # AnswerChannel, answer_bytes (pure)
crates/signaltty-agent/src/adapters/*.rs# answer_channel impls (Codex: TypeText; rest: default None)
crates/signaltty-server/src/params.rs   # HookEvent.decision, DecisionAnswer
crates/signaltty-server/src/store.rs    # set/answer/clear_decision transitions
crates/signaltty-server/src/router.rs   # ingest in hook-event, h_decision_answer
crates/signaltty-server/tests/integration.rs
crates/signaltty-cli/src/main.rs        # hook-event --decision, decision answer
crates/signaltty-gui/src/terminal.rs    # inline decision bar
docs/08-ipc.md  docs/02-data-model.md  docs/adr/0008-decision-answer.md
```

**Structure Decision**: follow existing crate seams; no new crates.

## Design

### Data model (core)

```rust
struct DecisionOption { id: String, label: String }
struct Decision {
    id: String, prompt: String, options: Vec<DecisionOption>,
    answerable: bool,          // set by server from adapter channel at ingest
    received_at: DateTime<Utc>,
}
Pane { pending_decision: Option<Decision> }  // skipped when None (old snapshots load fine)
```

`answerable` is data captured at ingest (which adapter owned the pane then),
not presentation logic leaking into core: GUI/CLI render buttons iff true.

### IPC contract (proto + docs/08)

- `decision.answer {pane_id, decision_id, option_id}` →
  `{answered: bool, lifecycle?, attention?}`. `answered:false` (ok) when the
  id is consumed/superseded (double-click, two clients, stale bar).
  Errors: `NO_SUCH_PANE`, `PANE_EXITED`, `NO_SUCH_DECISION` (unknown id AND
  no decision pending — caller bug, surfaces loudly), `BAD_PARAMS`
  (unknown option id, adapter without channel).
- Events `decision.created {pane_id, decision, prev?}`, `decision.answered
  {pane_id, decision_id, option_id}`, `decision.cleared {pane_id, decision_id,
  reason}`. `reason`: `attention_cleared | moved_on | pane_exited`
  (`answered` has its own event; supersedes fold into one `created`).
- `hook-event` accepts `decision: {id, prompt, options[{id,label}]}`.
  Present → set/supersede (emits created; supersede folds into one created
  with `prev` in payload — one emit per transition, never two). Absent →
  existing decision untouched (prose-only must not fake or drop buttons).
  Lifecycle leaving `Blocked` (or attention clearing to `None`) with a
  decision pending → `decision.cleared`, reason `attention_cleared`.

### Answer channel (agent)

```rust
enum AnswerChannel { TypeText }   // extend later: HookVerdict, StdinJson…
fn answer_bytes(channel, options, option_id) -> Option<Vec<u8>>
// TypeText: 1-based index of option_id + "\n"  ("1\n" for first)
trait AgentAdapter { fn answer_channel(&self) -> Option<AnswerChannel> { None } }
```

Codex returns `Some(TypeText)`; claude/opencode/cursor/generic keep the
default until probed. Delivery reuses the `pane.input` write path
(same bytes, same rate limits) — typed text is the channel, not a fallback.

### GUI (terminal.rs)

Persistent `decision_bar` (`Box`: prompt `Label` + options `Box` + hint)
between header and scroller, built once in `PaneWidget::new`, toggled in
`update_meta` from `pane.pending_decision` (VTE widget untouched → no
rebuild, no focus steal). Click → `decision.answer`; failure → toast.
Pure helper `decision_render(&Decision) -> (prompt, Vec<label>, hint_visible)`
is the headless-tested seam. Long text ellipsized with tooltips.

### CLI

- `hook-event --decision '<json>'` (object; passed through as `decision`).
- `decision answer --pane ID --decision DID --option OID [--json]`.
- `pane get` human output appends decision prompt + option labels.

## Probe note (replaces blocking Assumptions)

`codex`, `claude`, `opencode` binaries exist in this env, but live approval
prompts need an interactive TUI session — not drivable in this harness.
Fixture verification (bytes reach the PTY; agent resumes on real input) is
the green gate for v1; a live-Codex end-to-end (SC-001 literally) is filed
as follow-up in tasks.md and does not block merge.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| New `NO_SUCH_DECISION` code | Stale-id must error distinctly from `BAD_PARAMS` (caller bug vs shape bug) | Reusing `BAD_PARAMS` conflates shape and identity errors at the seam |
| `answerable` stored on `Decision` | GUI/CLI must render read-only without depending on `signaltty-agent` | GUI depending on agent crate breaks the gtk-only-GUI layering (ADR-0004) |
