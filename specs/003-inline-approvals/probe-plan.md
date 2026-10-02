# Implementation plan: inline approvals

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

**Feature:** `003-inline-approvals` | **Date:** 2026-10-02
**Spec:** [probe-spec.md](probe-spec.md)
**Status:** response-channel probe complete; production implementation not started.

## Summary

Keep Claude's terminal UI and add a synchronous permission-hook bridge.
The live probe demonstrated Once and Deny for Bash permissions, including
simultaneous signaltty panes. Register the pending decision with a held hook
connection; GUI and CLI clients answer through `decision.answer`.

The server owns arbitration and cleanup. The adapter interprets requests and
formats Claude verdicts. Core holds the serializable projection; the GUI
updates a decision bar without rebuilding VTE.
See [probe results](probe-results.md) and [research](research.md).

## Technical context

- Rust 1.85, edition 2021, using existing workspace dependencies.
- Linux, JSONL Unix sockets, Tokio one-shot responses, GTK4/libadwaita/VTE.
- Runtime responders stay in `signaltty-server`; core has no async or OS calls.
- One pending decision per pane, excluded from snapshot save and restore.
- Claude Bash permission requests offer Once and Deny. Other adapters and
  request kinds remain read-only until independently proven.
- Use `pane.updated` for decision changes so existing GUI invalidation works.
- Proposed deadlines: bridge 540 seconds, Claude command hook 600 seconds.
  Validate cancellation before release; normal observer hooks retain their
  short timeout. No lock survives an await.

## Constitution check

The existing spec records the verified channel and narrows the first capability
to proven Once/Deny Bash permissions. Always and question/plan responses require
separate probes. This staged delivery of directive 2 is justified in the spec.
No guessed policy or permission keystrokes are introduced.

Planning precedes tasks and production code. Implementation must load the
implement, TDD, applicable principle, and GUI skills before edits. New IPC params
are typed and use proto constants. State changes go through Store transitions
with paired events. Decision-bar updates preserve terminals.

[The probe design decision](probe-design-decision.md) records the proposed
transport and independent decision lifetime. No production code changed during
the probe. The post-design gates require the remaining behavior tests below.

## Grounding

The current flow is hook shim, CLI `Client::call`, `handle_conn`, router
classification, Store transitions, broadcasts, GUI refresh, and
`PaneWidget::update_meta`. The server owns processes and state.

`handle_conn` awaits `dispatch` inside its socket-read branch. A waiting handler
alone would prevent EOF detection. Return a pending-hook outcome to the
connection task instead; select over reply, EOF, and deadline there.

Focus calls `pane.mark_seen`, which clears attention. Focus must not consume
a decision. PTY exit, close, session termination, semantic resolution,
supersession, deadline, and transport loss retire the responder explicitly.
Snapshot persistence currently clones panes, so adding a decision field also
requires stripping it on save and restore.

## Architecture comparison and decision

A held connection registers a one-shot responder and exposes one public
answering method. Registration and publication happen under one Store lock.
Its lifetime gives cleanup when the hook helper dies.

Short polling requests preserve the current request-response loop but need an
owner token, result retention, a polling method, and a lease or process watcher.
A lease permits stale selections after hook death; a process watcher adds OS
machinery. Choose the held connection. Semantic resolution is necessary in
both designs because Claude may keep the hook alive after a terminal answer.

## Caller contract and type sketch

The proposed hook caller is `signaltty hook-event --agent claude --event
PermissionRequest --payload-stdin --wait-for-answer`. It emits only Claude
verdict JSON after a choice. Unsupported tools, cancellation, expiry, or an
unavailable server yield empty stdout and normal terminal handling.
Diagnostics go to stderr. Existing observer calls still return immediately.

UI and CLI callers use `decision.answer {pane_id, decision_id, option_id}`.
They never send Claude JSON or guessed permission keys.

```rust
// Core projection, without runtime handles.
struct Decision {
    id: String,
    prompt: String,
    options: Vec<DecisionOption>,
    received_at: DateTime<Utc>,
    answerable: bool,
}
struct DecisionOption { id: String, label: String }

// Adapter interprets tool-specific requests and output.
fn decision_request(event: &AdapterEvent) -> Option<DecisionDraft>;
fn answer_channel(event: &AdapterEvent) -> Option<AnswerChannel>;
fn hook_verdict(option_id: &str) -> Result<Value, AnswerError>;

// Server-only runtime state, under the existing Store lock.
struct PendingHook {
    decision_id: String,
    reply: oneshot::Sender<HookReply>,
}
enum HookReply { Choice(String), YieldToTerminal }
enum DispatchOutcome {
    Immediate(Response, ConnEffect),
    PendingHook(PendingHookReply),
}

fn request_decision(/* pane, draft, sender */) -> Result<Vec<StoredEvent>, Error>;
fn answer_decision(/* pane, decision id, option */) -> Result<AnswerReceipt, Error>;
fn cancel_decision(/* pane, decision id, reason */) -> Vec<StoredEvent>;
```

The sketch describes ownership. Establish final signatures with failing
behavior tests before production implementation.

## Arbitration and cancellation

Validate the live pane, decision ID, option, and responder under one Store
write lock. The first valid answer takes the sender and consumes the decision.
Keep bounded recent consumed IDs for duplicate retries. Duplicate requests
never queue another verdict; unknown or superseded IDs use `NO_SUCH_DECISION`.
A failed send clears the stale transport and returns an unavailable receipt.

Successful queueing is not an execution acknowledgement from Claude.
Connection cleanup matches both pane and decision ID, so an older connection
cannot cancel a newer request. Additional incoming bytes on a held hook socket
close that dedicated connection; general request multiplexing is out of scope.

The probe showed that a terminal answer can win while the hook remains alive.
Matching `PostToolUse` retires completed requests, but arrives after execution.
Before forwarding actual user bytes through the shared PTY input path, retire
any live decision and release the hook without a verdict. This conservatively
hands control to the terminal without interpreting keys or inventing an answer.
Apply it to GUI input and interactive CLI attachment. Focus and resize do not
trigger this transition. Test direct Yes, No, Escape, and a long-running command.

## GUI behavior

Insert the decision bar between the header and terminal scroller. Update it
through `PaneWidget::update_meta` without replacing VTE or the split tree.
Render supported options from the typed projection. Disable options while an
answer runs; refresh after accepted, duplicate, stale, or unavailable results.
Unsupported requests show a prompt and answer-in-terminal hint. An absent
decision adds no visible space. Preserve focus, selection, and terminal contents.

## Planned files

- Core `model.rs`: decision projection and pure state tests.
- Agent `types.rs` and `adapters/claude.rs`: capability and verdict handling.
- Proto `lib.rs`: method and error constants, included in schema lists.
- Server `params.rs`, `router.rs`, and `store.rs`: typed requests and transitions.
- Server `server.rs`: pending connection response, EOF, and deadline handling.
- Server `pty.rs`: input and exit invalidation; router pane close also cancels.
- Server `persist.rs`: strip ephemeral decisions.
- Server `tests/integration.rs`: arbitration, lifetime, and restore tests.
- CLI `main.rs` and `integration.rs`: waiting helper, answer command, and explicit
  permission-hook installation. Keep personal config merging non-destructive.
- GUI `terminal.rs` and `app_tests.rs`: structural bar and display behavior.
- Docs `02`, `07`, and `08`: implemented model, hooks, and contract rows.

## Verification before release

Create tasks from this plan, then use red-green-refactor. Cover two clients,
repeat answers, stale IDs, supersession, timeout, hook disconnect, terminal
input, pane exit, and restart. Input invalidation must occur before PTY writing,
while focus and resize preserve decisions. Bound duplicate history and validate
malformed options, unknown tools, and payload size limits.

Repeat the native experiment with the production helper. Verify a real GUI
click resumes Claude. Record light and dark screenshots and assert stable VTE
identity as the bar appears and disappears. Run workspace tests, fmt, and
clippy. Current experiment results do not establish GUI support or server-side
answer arbitration.
