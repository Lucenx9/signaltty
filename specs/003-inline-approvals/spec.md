# Feature Specification: Inline Approvals

**Feature Branch**: `003-inline-approvals`

**Created**: 2026-09-29

**Status**: Design (implementation deferred — see Assumptions)

**Input**: User description: "docs/14 directive 2 requires inline approvals with no context-switch (cmux Feed: permission Once/Always/Deny, ExitPlanMode, AskUserQuestion); today the GUI only shows an 'Approval' pill and the user must answer inside the agent's TUI"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Answer where you look (Priority: P1)

When an agent needs a decision, the pane card shows the question and
its options inline (below the header, above the terminal); the user
clicks an answer (or presses its shortcut) without focusing the
terminal or learning the agent's TUI keys. The card collapses back to
the plain terminal once answered.

**Why this priority**: This is directive 2 verbatim — "render the
decision where the user already looks". Everything else (history,
policies) is secondary.

**Independent Test**: Fixture a pane with a pending decision; assert
the decision bar renders prompt + options; click one; assert the
answer reaches the pane (integration: bytes the agent accepts) and
the bar clears when attention clears.

**Acceptance Scenarios**:

1. **Given** a pane with attention `permission_required` and a pending
   decision (prompt + options Once/Always/Deny), **When** the pane
   renders, **Then** the question and three answers are visible
   inline without focusing the terminal.
2. **Given** a pending decision, **When** the user clicks "Once",
   **Then** the agent receives the equivalent of answering Once in
   its own TUI, and the bar disappears when the agent resumes
   (attention clears).
3. **Given** a pending decision, **When** the user ignores it and
   answers inside the terminal instead, **Then** the bar clears on
   the next attention-cleared event (no stuck UI, no double answer).

---

### User Story 2 - Structured decision requests (Priority: P2)

Hook shims and `hook-event` carry decisions as data
(`decision: {id, prompt, options[]}`), not prose: the GUI renders
options as buttons, the CLI prints them as a list, plugins can react
to them. Prose-only requests (legacy shims, OSC/title fallbacks)
render as a single "Open pane" hint, never as fake buttons.

**Why this priority**: Directive 3 — detection as data. Buttons must
mean something; unstructured text must never render as options.

**Independent Test**: `hook-event` with a decision payload stores it
on the pane; `pane.get` returns it; unknown fields ignored; a
prose-only event yields no decision object.

**Acceptance Scenarios**:

1. **Given** a `hook-event` with
   `decision: {id, prompt, options: [{id, label}]}`,
   **When** the GUI refreshes, **Then** each option renders as one
   button labelled exactly `label`.
2. **Given** a `hook-event` with only `message` (no decision),
   **When** the GUI refreshes, **Then** no decision bar appears; the
   existing attention pill + message row are unchanged.

---

### User Story 3 - One answer channel per adapter (Priority: P3)

Each agent adapter declares how an answer is delivered
(`answer_channel`: typed text, stdin JSON, hook verdict, …) and the
server translates "user picked option X" into that channel. Adapters
without a proven channel render decisions as read-only (prompt +
"answer in the terminal" hint).

**Why this priority**: Answering wrong (typing bytes an agent
misreads) is worse than not answering. Channels must be probed
against real CLIs, one adapter at a time, starting with whichever
probe succeeds first.

**Independent Test**: Per-adapter fixture: decision + option →
expected delivery bytes/payload; adapter without a channel →
read-only rendering.

**Acceptance Scenarios**:

1. **Given** a decision on a pane whose adapter has no proven
   channel, **When** rendered, **Then** the prompt shows with no
   clickable options and a hint to answer in the terminal.
2. **Given** a decision on a pane with a proven channel,
   **When** the user picks an option, **Then** delivery uses that
   channel and nothing else (no blind `pane.input` fallback).

---

### Edge Cases

- Decision arrives for an exited pane: stored, rendered read-only
  (nothing can receive the answer).
- Two decisions for one pane: latest wins; the older id is dropped
  with no error (agents supersede prompts).
- Answer clicked twice (double click / two clients): second delivery
  is a no-op once the decision id is consumed or attention cleared.
- Decision id unknown to the server (stale client): `NO_SUCH_*`-style
  error, GUI refreshes and drops the bar.
- Long prompts/options: ellipsized with full text in tooltips
  (existing `has_label`/tooltip pattern).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The server MUST model at most one pending decision per
  pane (`{id, prompt, options[{id, label}], received_at}`), set by
  `hook-event`, cleared when attention clears or the pane exits.
- **FR-002**: The server MUST expose one answering method
  (`decision.answer {pane_id, decision_id, option_id}`) that validates
  ids, delivers through the pane adapter's channel, and consumes the
  decision id (repeat answers are no-ops, not errors).
- **FR-003**: The GUI MUST render a pending decision as an inline bar
  (prompt + one button per option, keyboard reachable) that appears
  and clears with the pane's decision state; it MUST NOT rebuild the
  terminal when the bar toggles (structural reconcile).
- **FR-004**: Adapters MUST declare their answer channel in code
  (`AgentAdapter::answer_channel`) with exactly one initial
  implementation: whichever adapter probe succeeds first during the
  plan phase; all others render read-only until probed.
- **FR-005**: Prose-only attention (no decision object) MUST NOT
  render option buttons anywhere.

### Key Entities

- **Decision**: `{id, prompt, options[{id, label}], received_at}` —
  one pending per pane, superseded by newer, consumed on answer.
- **AnswerChannel**: per-adapter delivery strategy (typed text, hook
  verdict, …) — absent until proven against the real CLI.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For the first probed adapter, a user answers a live
  permission prompt by clicking one inline button, and the agent
  resumes with that choice (end-to-end, real CLI).
- **SC-002**: Double-answer and stale-id cases never deliver twice
  and never error loudly (covered by integration tests).
- **SC-003**: Panes without decisions render byte-identical to today
  (no layout shift, no new chrome).

## Assumptions

- **Deferred**: this spec ships as design only. Implementation waits
  for the plan-phase adapter probes (which CLIs accept answers
  non-interactively, and how) because FR-004 forbids guessing
  channels. The probes are small (one evening against installed
  CLIs) but must be empirical, not speculative.
- Likely first channel candidates, in probe order: Claude Code
  `PreToolUse` hook verdict (documented decision mechanism) →
  Codex TUI typed answer (`1`/`2`/… + Enter over `pane.input`) →
  opencode/cursor equivalents. Order may change on evidence.
- `decision.answer` delivery reuses the `pane.input` path when the
  channel is typed text (same bytes, same rate limits), and only
  then.
- No Always/Deny persistence in v1: "Always" answers this session's
  prompt once (true policy storage is a later spec).
- Directive 2's ExitPlanMode/AskUserQuestion variants are the same
  mechanism with different prompts — no separate UI.
