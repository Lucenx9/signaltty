# Agent Orchestrator orchestration review, 2026-10-12

The strongest ideas to adapt are acknowledged report delivery, reactions to CI and review events, and richer PR facts. They improve coordination after a worker starts. A persistent project conversation is a separate product extension.

## Scope and evidence

Reviewed upstream `main` at [1787604f](https://github.com/OrchestratorInc/agent-orchestrator/tree/1787604f1524e027db880b3e4e0a6b98fb2375c7), committed 2026-10-11 22:52:09 UTC. The repository was shallow-cloned into `/tmp/signaltty-agent-orchestrator-research`. No upstream code or tests were executed.

This note rechecks selected mechanisms from the [October 4 investigation](../../specs/018-orchestrator/research-agent-orchestrator.md). It does not establish when those mechanisms were introduced. The current source confirms the report, reaction, generation, and surviving-runtime patterns described there. Product statements below are marked documented. Other upstream observations come from inspected code. Recommendations are design proposals, not runtime results.

## Reports need an acceptance boundary

AO persists report delivery separately from the worker's reported outcome. The coordinator claims a batch, submits it with an idempotency key, and acknowledges it only after `Delivery.Submit` succeeds. A failed submission releases the claim. Urgent `stuck` reports can interrupt the orchestrator, with a durable three-minute limit per worker. See [report coordinator](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/service/report/coordinator.go) and its [acceptance and stale-token tests](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/service/report/coordinator_test.go).

For a TUI target, `SendSemantic` waits up to ten seconds for a matching coordination checkpoint. Writing bytes to the pane alone is insufficient. Chat delivery has a caller-supplied idempotency key intended to prevent duplicate turns after acceptance but before outbox acknowledgment. See [SendSemantic](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/session_manager/manager.go) and [RelayChatTurnWithID](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/service/chat/service.go).

Recommendation: keep a report's content and its delivery state separate. Persist the intended recipient, claim, attempts, and acknowledgment. Reuse one delivery identifier across retries. Test a crash after acceptance and before acknowledgment. State the guarantee per adapter: an observed prompt submission does not prove that the model understood or acted on the report.

AO routes batches to the newest live project orchestrator, rather than a stored parent. That routing is explicit in `activeOrchestrator`. Preserve signaltty's task lineage when adapting delivery. A project coordinator can be an optional recipient, but it should not replace a task's owning parent silently.

## Reactions make PR feedback actionable

AO's `ApplyPRObservation` queues independent CI, review-comment, review-decision, and merge-conflict nudges. It persists deduplication signatures and attempt counts. Ordinary CI and review nudges pause while the worker needs input. The urgent conflict route still guards permission dialogs. See [PR reactions](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/lifecycle/reactions.go).

The review budget is three nudges. CI uses `maxAttempts: 0`, so this mechanism does not establish a uniformly bounded CI retry policy. Recommendation: define explicit attempt and time budgets for each automatic reaction, then escalate exhausted or repeated failures to the user. Show the last trigger, action, and reason for stopping.

Keep signaltty's terminal task states immutable. A CI failure after completion should create a linked follow-up task, or a separately modeled execution attempt if a future spec introduces that concept. Do not reopen the completed task implicitly.

## PR readiness needs richer, current facts

AO's PR model includes `Draft`, `Mergeability`, and `HeadSHA`. Its session service consumes current-head review runs, and board status derives from facts. See [PR facts](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/domain/pr.go), [current-head review projection](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/service/session/kanban.go), and [board derivation](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/pkg/contract/kanban.go).

signaltty's [`TaskPr`](../../crates/signaltty-core/src/model.rs) currently stores number, URL, state, checks, review, and `checked_at`. Its [`gh pr view` refresh](../../crates/signaltty-server/src/tasks.rs) requests `state,reviewDecision,statusCheckRollup`. The GUI [`board_column`](../../crates/signaltty-gui/src/board.rs) accepts any recorded check time plus passing or absent checks and approved or absent review as “Ready to merge”. Those facts do not establish draft status, mergeability, or whether the result belongs to the current head.

Recommendation: store the observed head SHA, draft status, mergeability, observation time, and fetch outcome. Derive readiness in core so GUI and CLI agree. A new head or stale observation needs another refresh before a positive readiness claim. Unknown mergeability should stay visible as unknown. Test draft PRs, a changed head after green CI, stale checks, and a refresh failure.

Do not copy AO's board labels as merge authorization. Its derivation can put an externally approved PR in `Ready` before inspecting mergeability. Its board communicates ownership of the next step, which is a different contract from a guarantee that merge is safe.

## Runtime generation prevents old agents from changing current state

AO checks TUI callbacks against `RuntimeLaunchID` and Chat callbacks against `ControllerGeneration`. Mismatched callbacks return without changing lifecycle state. During a launch, hooks can wait for the new generation to commit. See [generation checks and PrepareLaunch](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/lifecycle/manager.go).

Recommendation: when adding restart or replacement, bind callbacks, report delivery, waits, and pending decisions to the execution that created them. Test late completion from the previous process and a callback arriving during launch. A task identifier alone cannot identify which execution owns an event.

AO deliberately keeps sessions alive across daemon exit and adopts them on the next boot. Detached provider hosts also preserve in-flight work. See [daemon shutdown behavior](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/backend/internal/daemon/daemon.go). Preserving live PTYs across server replacement is a distinct architectural project. Durable task metadata alone cannot provide that guarantee.

## A project conversation can preserve planning decisions

Documented: the [README](https://github.com/OrchestratorInc/agent-orchestrator/blob/1787604f1524e027db880b3e4e0a6b98fb2375c7/README.md) describes a persistent project orchestrator conversation that keeps goals, decisions, constraints, and planning history alongside live workers and PR state. This note does not verify the quality of model recall or automatic context selection.

Recommendation: begin with an editable project brief linked to task lineage and artifacts. Keep observed task facts separate from planning prose, and attach relevant decisions when delegating. This is useful once repeated planning loses context across tasks. It does not require a new autonomous planning agent as the first step.

## Suggested order

First correct PR readiness facts and shared derivation. Then add acknowledged report delivery and bounded feedback reactions. Add execution generations before introducing process replacement or retries that reuse a session. Treat persistent project planning and surviving PTYs as separately specified work.

Verification for this note is source inspection and `git diff --check`. No runtime, performance, or provider reliability result is claimed.
