# Improvements from the cmux and Herdr source code

Checked 2026-10-03. The strongest improvements for signaltty are reliable event
recovery, waits tied to new agent activity, and foreground-aware detection.
These improve the existing local workflow before adding more product scope.

## Scope and evidence

This is a source review, not a runtime comparison. The reviewed revisions are:

- signaltty `de84caf26c1ec0191569a9a6dce85fb83d5e4bb1`.
- [cmux `1a7d5cd`](https://github.com/manaflow-ai/cmux/tree/1a7d5cd08ad677359c0a273c10e13b853b86da31).
- [Herdr `5da0a01`](https://github.com/herdrdev/herdr/tree/5da0a01e1eedda054db0c81dd3a780000c40d9f0).

The upstream repositories were shallow-cloned outside this checkout. Links pin
the inspected source rather than a moving branch. No upstream app or test suite
was executed. Local defects below follow from the code paths inspected; their
runtime impact still needs regression tests when implemented.

The [September 30 comparison](2026-09-30-cmux-herdr.md) describes the broader
products. Its local inventory is historical: signaltty now has native Claude and
Codex permission replies, worktree management, command-palette navigation, and
per-file diff review. See [agent integration](../07-agents.md),
[local workflows](../../specs/013-local-agent-workflows/spec.md), and
[file diff review](../../specs/014-file-diff-review/spec.md).

## 1. Make event cursors survive restart and report lost history

cmux allocates persistent sequence ranges through
[`CmuxEventSequenceStore`](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/Sources/CmuxEventSequenceStore.swift#L43-L93).
Its subscription acknowledgment includes the retained range, a boot identifier,
and an explicit gap indication.
[`CmuxEventBus`](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/Sources/CmuxEventBus.swift#L479-L514)
also closes a subscription whose pending live events exceed its bound, instead
of silently continuing after loss.
[The restart test](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/cmuxTests/CmuxEventBusTests.swift#L82-L104)
checks that newly published events advance beyond the previously reserved range.

signaltty's [`Store::new`](../../crates/signaltty-server/src/store.rs) sets `seq`
to zero. [`Snapshot`](../../crates/signaltty-server/src/persist.rs) does not save
or restore the sequence, while the audit file survives restart. A new event can
reuse an old sequence. `AuditLog::read_since` can then filter out new events
behind an old cursor, and `merge_replay` collapses different events with the
same sequence. The existing `audit_replays_across_restart` test reads an old
event after restart but does not publish a new event before checking replay.
See [audit implementation](../../crates/signaltty-server/src/audit.rs) and
[integration tests](../../crates/signaltty-server/tests/integration.rs).

There is a second recovery gap. The shared broadcast holds 4096 events,
including PTY output. [`handle_conn`](../../crates/signaltty-server/src/server.rs)
handles `RecvError::Lagged` with `continue`. The GUI
[`deliver_event`](../../crates/signaltty-gui/src/actor.rs) removes overlapping
bytes but accepts a forward byte-offset gap without requesting a new screen.
Replay is capped at 4096 records without a continuation or truncation indicator.
These paths can leave a connected client missing state or terminal bytes.

Recommendation: define cursor continuity, retained-range metadata, and a loss
response together. Recover state through replay or a fresh snapshot, and
recover PTY output through the existing attach snapshot. Distinguish state
events from terminal traffic so a noisy pane cannot silently erase approvals.
Do not infer loss from consecutive event numbers: PTY events consume numbers,
and subscription filters also create legitimate gaps.

Herdr provides a smaller useful example. Its volatile ring detects a cursor
older than retained history and returns `events_lost`, directing the caller to
resubscribe and obtain `session.snapshot`.
[History check](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/api/event_hub.rs#L46-L66),
[subscription error](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/api/subscriptions.rs#L326-L337).
This is a loss-detection example, not persistent replay across server boots.

Acceptance cases: publish before and after restart; replay beyond the cap;
resume outside retention; flood PTY output while a slow client awaits an
approval; repair a terminal byte gap without rebuilding its widget.

## 2. Tie prompt and wait to the work just submitted

Herdr's `agent.prompt` with a wait first verifies new activity. It then waits
for the requested settled state, using the remaining overall timeout. It also
checks terminal and agent identity while waiting.
[Activity gate](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/api/wait.rs#L250-L325),
[identity validation](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/api/wait.rs#L453-L506).
Its connection checks allow abandoned waits to terminate. Herdr's plain
`agent.wait` still accepts an already-matching state; the activity gate belongs
specifically to prompt-and-wait.

signaltty's [`wait`](../../crates/signaltty-server/src/router.rs) checks only the
pane's current lifecycle or attention. The
[`Wait` params](../../crates/signaltty-server/src/params.rs) have no turn,
generation, or transition cursor. If a reused agent is still `done` when new
input is submitted, `wait --until done` can return before its next working hook.
This is valid current-state behavior but insufficient for waiting on new work.
Ordinary waits also lack the disconnect/shutdown cancellation used by native
permission reporters.

Recommendation: keep current-state waits and add an explicit new-work contract.
A prompt operation can return a work token, or a wait can require a transition
after a supplied cursor. Validate pane process/session identity, support waiting
for any of `done`, `blocked`, or `failed`, and cancel disconnected callers.
The [agent skill](../../crates/signaltty-cli/assets/SKILL.md) should teach the
new-work path for repeated orchestration.

Acceptance cases: old `done` state, very fast completion, approval immediately
after submission, process replacement, failed submission, and caller disconnect.
Do not copy Herdr's wait code wholesale: its internal wait event reads use the
unchecked history accessor, so its subscription loss handling is not applied
uniformly to every wait.

## 3. Detect the foreground agent and explain fallback state

Herdr uses the terminal foreground process group on Linux. Its detector prefers
a recognized process-group leader over agent-like child processes, preventing
an MCP child from replacing its parent's agent identity.
[Linux foreground lookup](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/platform/linux.rs#L651-L665),
[leader preference](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/detect/mod.rs#L249-L261).

signaltty's [`deepest_descendant`](../../crates/signaltty-server/src/procscan.rs)
follows the highest child PID through known shells. That is not a foreground
job check. `scan` only promotes generic panes, so detection alone cannot update
an already-recognized pane when its shell starts a different agent.

Herdr also describes screen evidence in versioned TOML rules, including screen
regions, priorities, and exclusions for historical transcript views.
[Codex rules](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/detect/manifests/codex.toml),
[reload and explain implementation](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/detect/manifest.rs).
signaltty's [manifests](../../crates/signaltty-agent/src/manifest.rs) extend binary
names, hook mappings, and resume metadata. They do not classify screens and are
loaded once. The server already owns a
[headless screen model](../../crates/signaltty-term/src/headless.rs), so fallback
detection does not need to scrape GUI widgets.

Recommendation: first fix foreground process attribution. Then add a small
server-side fallback rule engine for agents without semantic signals, with
`explain` output reporting rule, evidence source, and manifest version. Preserve
hook precedence and `unknown` when evidence is insufficient. Initially support
validated local reloads; a remote rule-update service adds separate scope.

Acceptance cases: background jobs, an agent-like MCP child, switching agents in
one shell, old approval text in scrollback, transcript viewer mode, and a hook
that overrides contradictory screen evidence.

## 4. Expand decisions through provider capabilities

cmux's
[`FeedEventClassifier`](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/CLI/FeedEventClassifier.swift#L90-L105)
distinguishes permission requests, plan approval, questions, and telemetry.
Actionability depends on the provider event channel. A blocked-looking event
does not automatically create an answerable card.

signaltty already has a real native permission route with pane/session/deadline
validation, transient response delivery, and timeout fallback. Its shared
[`permission` contract](../../crates/signaltty-agent/src/permission.rs) offers
Allow once and Deny. Preserve that working path. The next useful extension is
typed questions and plan review with provider-specific response translation,
not additional generic buttons on the existing permission card.
See [native routes](../../crates/signaltty-server/src/approvals.rs).

Recommendation: represent decision kind and supported response capabilities
explicitly. Add question and plan bridges only where the provider offers a
verified reply contract. Keep telemetry read-only and expose expiry/fallback in
the UI. cmux's
[Codex approval bridge](https://github.com/manaflow-ai/cmux/blob/1a7d5cd08ad677359c0a273c10e13b853b86da31/CLI/CodexTeamsApprovalBridge.swift#L199-L278)
shows how to translate the provider's available choices rather than assume
one response vocabulary. Do not offer Always to Codex while its verified hook response cannot
persist permission updates. Provider support must be checked again during
implementation; the [current integration doc](../07-agents.md) records the
versions tested locally.

Acceptance cases: unsupported decision kind, multi-question answers, expired
request, duplicate click, simultaneous panes, and a provider that handles the
decision in its own terminal before the GUI replies.

## 5. Align snapshot durability with the documented guarantee

signaltty's [persistence doc](../09-persistence.md) promises temporary write,
fsync, and rename. Its [`atomic_write`](../../crates/signaltty-server/src/persist.rs)
does `fs::write`, permissions, and rename without syncing the file or directory.
The implementation therefore does not establish the documented power-loss
durability. A corrupt snapshot is renamed aside and startup begins empty.

Herdr's
[`SessionWriter`](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/persist/writer.rs)
preserves recovery copies for specific unreadable or incompatible session cases.
Its [ordinary JSON writer](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/persist/io.rs#L48-L61)
also uses write and rename, so it is not evidence for an fsync guarantee.

Recommendation: implement the stated durability or narrow the documentation.
Consider retaining one validated previous snapshot for recovery, with a visible
restore error and a bounded retention policy. Protect an unreadable or newer
snapshot from replacement until a recovery copy succeeds, as Herdr's
[writer guard](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/persist/writer.rs#L14-L75)
does. Keep explicit session resume and
the distinction between restored layout and surviving processes.

Acceptance cases: malformed current snapshot, incompatible version, failed
write/rename, interrupted save, and native decisions restored as unanswerable.
Power-loss durability needs a separate fault-injection proof; an ordinary
restart test cannot establish it.

## Priority and limits

Implement event continuity and recovery first, then new-work waits, then
foreground attribution and fallback detection. Those changes make the existing
parallel-agent workflow dependable. Typed questions and plan review are the
next product extension. Snapshot durability is an independent correctness fix.
This ordering is a recommendation, not a benchmark result.

Per-client completion visibility is useful when multiple clients become a real
workflow. Herdr's
[client acknowledgment](https://github.com/herdrdev/herdr/blob/5da0a01e1eedda054db0c81dd3a780000c40d9f0/src/client/shell/endpoint_agent_state.rs#L118-L175)
checks focus and the boot/revision of the frame actually presented. That is also
useful as a guard against acknowledging an old completion. signaltty currently
keeps [`last_seen_at` and attention](../../crates/signaltty-core/src/model.rs)
on the server pane, and [`mark_seen`](../../crates/signaltty-server/src/store.rs)
clears ordinary attention globally. Keep that deliberate model until a
multi-client requirement justifies separate reading state. Pending decisions
already survive acknowledgment.

Browser, managed SSH, and remote machines remain larger extensions. This review
does not change the [native GTK architecture](../06-gui-toolkit.md) or the
[product direction](../14-product-direction.md), and it does not propose copying
either repository's implementation wholesale. Feature implementation still
requires the project's spec, plan, tasks, and relevant regression tests.
