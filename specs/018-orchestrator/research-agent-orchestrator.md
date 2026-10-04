# 018 research: Untrivial agent-orchestrator

Studied source at `/tmp/agent-orchestrator`, commit `b2461dc5955bb11b4ee1213871e6a2157e68da0e` (2026-10-04 04:11:46Z, `fix: align macOS titlebar across zoom and sidebar transitions (#6192)`). Shallow clone; history and stars come from the GitHub API, not the clone. Claims are marked **VERIFIED-IN-SOURCE**, **FROM-DOCS**, or **INFERRED**.

Compared with `specs/018-orchestrator/spec.md`, `contracts/ipc.md`, `data-model.md`, and `docs/adr/0020-task-orchestration.md`.

## 1. What it is

**FROM-DOCS** (`README.md`): local desktop workspace that plans work, runs one coding agent per task in an isolated workspace, and supervises workers, pull requests, CI, and review on a live Kanban. Product name Agent Orchestrator (AO). Docs site `https://docs.aoagents.dev`. Apache-2.0.

**VERIFIED-IN-SOURCE** (`docs/architecture.md`, `backend/internal/domain/session.go`, `backend/internal/daemon/daemon.go`): a long-running Go daemon on `127.0.0.1` plus an Electron/React desktop, an Expo mobile client, and an `ao` CLI. The work unit is a **session**, not a separate task row. Two kinds: `worker` and `orchestrator` (`domain.KindWorker`, `domain.KindOrchestrator`). Two interface modes, never both live: `tui` (agent inside tmux or Windows conpty) and `chat` (native protocol controller, no agent terminal). Project sessions get git worktrees. Projectless workers get a plain directory (`scratch` adapter).

**VERIFIED-IN-SOURCE** (`backend/internal/adapters/agent/registry/registry.go` `Constructors`): 33 harness adapters are compiled in (claude-code, codex, opencode, opencode-v2, grok, cursor, qwen, gemini, copilot, kimi, muse, droid, amp, agy, crush, aider, goose, auggie, continue, devin, omp, cline, kiro, kilocode, vibe, pi, kimchi, prime-agent, autohand, fx, unreal-agent, mimo-code, deepseek-harness). **FROM-DOCS** README says "32 coding agents". The HTTP enum on `DelegateTaskRequest` also lists `fake`, which is not in `Constructors`.

**Maturity (GitHub API, 2026-10-04, not in the clone):** created 2026-02-13, default branch `main`, not archived, primary language Go. About 3067 commits (commits API `Link` last page with `per_page=1`). Stars 12703, forks 1753, open issues 747. Latest stable release `v0.13.3` (2026-10-01); nightlies through `v0.13.4-nightly.202610040352` the same morning as HEAD. Pushed 2026-10-04. This is a shipping product, not a sketch.

Architecture rule, **VERIFIED-IN-SOURCE** and **FROM-DOCS** (`docs/architecture.md`, `domain/session.go`): durable facts are `activity_state`, `is_terminated`, controller generation, and PR rows. Display status (`working`, `needs_input`, `ci_failed`, `mergeable`, Kanban column) is computed at read time and is not stored. Pipeline is observe → update durable facts → derive. SQLite plus a `change_log` CDC poller fans out SSE. Observation loops (SCM, runtime reaper, activity observer) do not write session rows directly; lifecycle reduces facts.

## 2. Task / work-unit model

There is no A2A task entity. A session row is the work unit.

**VERIFIED-IN-SOURCE** `SessionRecord` (`backend/internal/domain/session.go`): id, project, optional issue, kind, harness, reviewer harness, mode (`chat`|`tui`), activity `{state, lastActivityAt}`, `firstSignalAt`, `isTerminated`, `terminateOnPrMerge`, auto-inject flags, metadata, `cleanupGeneration`, revision, provision state. Metadata holds branch, workspace path, `diffBaseSha`, `diffBaseRef`, runtime handle and launch id, agent session id, prompt, latest user prompt, native transcript path, provider conversation id, controller generation, model, effort, preview URL. `parentSessionId` is **not** a column.

Activity states (`backend/internal/domain/activity.go`): `active`, `idle`, `waiting_input`, `blocked`, `exited`. `waiting_input` and `blocked` are sticky (time must not demote them). Both render as needs-input, but automation differs: `waiting_input` is an empty prompt and may be messaged; `blocked` is a pending permission/approval and must never receive injected input.

Display status (`backend/internal/domain/status.go`, derived in `backend/pkg/contract/kanban.go` and `backend/internal/service/session/status.go`): includes `no_signal` when a live TUI has had no hook for `noSignalGrace` = 90s. Kanban columns are `building | validating | needs_review | ready | archive`, derived from activity plus PR/CI/review facts.

Provision state (`session.go`): Chat spawn can return while the row is `provisioning`; messages queue until the controller exists; `failed` keeps the row and the queue. Restart of a still-`provisioning` row is marked failed with "AO restarted before this session finished starting" (`session_manager/manager.go` `ReconcileBackground`). Hidden `IsTaskPreparation` rows reserve a speculative worktree for the New Task dialog and are deleted if still unclaimed at boot.

Reports (`backend/internal/domain/report.go`) are a second durable object. States: `checkpoint | needs_input | stuck | done`. Outputs: `artifact | pr_created | pr_reviewed`. Delivery: `pending → claimed → acknowledged`. A `done` report opens a 5-minute settlement window (`ReportSettlementWindow`). Comment in `ReportRecord`: state and outputs do not change authoritative session state. `--done` does not terminate the session (**VERIFIED-IN-SOURCE** worker prompt text in `session_manager/prompt.go`).

**Restart (VERIFIED-IN-SOURCE):**

- Daemon shutdown does **not** tear sessions down. Comment in `backend/internal/daemon/daemon.go`: tmux/conpty sessions and detached chat hosts survive; next boot `Reconcile` adopts them and keeps session ids.
- `ReconcileStartupSafety` closes interrupted agent-switches and interface handoffs before the API accepts input, and fails interrupted provisioning.
- Runtime reaper (`backend/internal/observe/reaper/reaper.go`) polls every 5s. It reports facts only. If at least 5 sessions are probed and more than half look dead in one pass, the mass-death breaker downgrades those to failed probes and does not archive the board.
- Lifecycle (`backend/internal/lifecycle/runtime.go`, `manager.go` `ApplyRuntimeObservation`): a probe of `dead` terminates the session only when activity is not sticky and last activity is older than 60s. Sticky `waiting_input`/`blocked` therefore blocks termination even if the probe says dead. Supervised-process death with a live pane sets `activity=exited` but does not set `isTerminated`. Chat sessions are not runtime-probed; their host reports its own health.
- Cleanup disposition is its own row (`domain/cleanup.go`): `pending | removed | preserved_dirty | failed | not_applicable`, with attempt count and next-attempt time. Dirty worktrees are kept; other failures retry until `failed`.

**INFERRED:** a blocked session whose tmux session is actually gone can sit non-terminated forever, because stickiness makes `runtimeClearlyDead` false. That is the conservative false-death guard, and it is also a limbo mode 018 should not copy.

## 3. Spawn, isolation, base ref, cleanup

**VERIFIED-IN-SOURCE** `Manager.Spawn` (`backend/internal/session_manager/manager.go`): reject unknown harness, bad model, and explicit Chat preflight failures **before** any row. Then seed the session row, build prompts, create the worktree, provision (symlinks, post-create commands), write attachments into the worktree, install hooks, launch. Failures roll the seed row back or park it terminated. `clientRequestId` plus a body hash makes a retry return the same row only after `ClientRequestCommitted`; an incomplete prior launch is an error, not a second spawn.

`ParentSessionID` (`backend/internal/ports/session.go`, `backend/internal/cli/spawn.go`): `ao spawn` inside a session sends `AO_SESSION_ID` as `parentSessionId`. The daemon uses it only to inherit permission mode when the caller did not set one. It is not stored on the child. There is no parent/root/relationship column.

**Isolation (VERIFIED-IN-SOURCE):**

- Git projects: `git worktree add -b <branch> <path> <seedRef>` (`backend/internal/adapters/workspace/gitworktree/workspace.go`, `commands.go`). Branch already checked out elsewhere is a typed conflict. A missing-but-registered path is retried with `--force` on the existing-branch form, because `git worktree add -b` creates the ref **before** path validation (comment cites git 2.54). Failed fresh adds roll back the worktree and delete the branch only at the creation SHA (`rollbackPreparedAdd`).
- Default branch is not the source checkout's HEAD. `gitdefault.Resolver` (`backend/internal/gitdefault/resolver.go`) refuses to guess from HEAD. Order: live remote HEAD, cached remote HEAD, else `ao.defaultBranch` for repos AO itself initialized. Candidates prefer `origin/<default>` then `refs/heads/<default>` (`configuredBaseRefCandidates`).
- Seed ref and diff base are different (`worktreeRefs` in `workspace.go`). If `origin/<requested-branch>` exists it can seed the new branch, but the comparison ref stays the repo default so a fetched feature branch does not hide its own commits.
- Spawn refresh (`refreshDefaultBranchesBestEffort`): resolve refs first, then one 5s `FetchDefaultBranch` budget for the whole workspace. Fetch failure logs and continues with local refs. It is not Conductor's always-fetch, and it is not "local HEAD only".
- After create, `resolveSpawnDiffBase` (`manager.go`) stores `git merge-base HEAD <default-ref>`, else `HEAD`. The stored SHA is the merge-base against the default ref, not "whatever commit we branched from" if those differ.
- Scratch projects: plain directory, no branch (`ErrScratchBranchUnsupported` if a branch is requested).
- Env injected into the agent: `AO_SESSION_ID`, `AO_PROJECT_ID`, `AO_DATA_DIR`, `AO_RUNTIME_LAUNCH_ID`, `AO_PERMISSION_MODE`, plus a supervised-process marker so tmux does not fall through to an interactive shell (`manager.go` env constants). Docker containers the worker starts can be labeled `ao.session=$AO_SESSION_ID` and are reaped on terminal; `ao.spare=true` opts out (comment, issue #2652).
- Speculative worktree: `POST /projects/{id}/tasks/prepare` while the New Task dialog is open; cancel or boot reclaims it (`delegation.go`, `ReconcileBackground`).

**Launch mode (VERIFIED-IN-SOURCE):**

- TUI: interactive CLI argv inside tmux/conpty. Claude shape is `claude [--session-id] [--permission-mode] [--append-system-prompt-file] -- <prompt>` (`claudecode/claudecode.go`). Prompt delivery strategies (`ports/agent.go`): `in_command` (default, `agentbase.go`), `after_start` (Aider injects after the TUI is up), `custom_agent`.
- Chat: no PTY. Detached per-session host. Codex app-server and ACP drivers live there so a daemon replacement can reconnect mid-turn (`docs/architecture.md`, `domain/session.go` controller generation). `Async: true` on desktop delegate returns at `provisioning`.
- `DelegateTask` (`backend/internal/service/session/delegation.go`) is the desktop "new task" path: it spawns a **worker directly**. `OrchestratorID` in the response is optional and unused by this function. The orchestrator agent delegates by running `ao spawn` / `ao send` from inside its own session (system prompt in `prompt.go`; Copilot gets an extra directive wrapped around every orchestrator send in `copilotOrchestratorMessage`).

**Cleanup (VERIFIED-IN-SOURCE `gitworktree/discard.go`, `workspace.go`):**

- Normal remove refuses a dirty tree (`ports.ErrWorkspaceDirty`) using porcelain, not English stderr (`isDirty` = `git status --porcelain` non-empty).
- Fast path renames the directory into `<managed>/.discarded/` **first**, then checks dirt at the new path, and renames it back if dirty. The comment explains why: a check-then-delete races a stray writer. Unlink of a clean tree runs in the background; leftovers are swept on next boot. `git worktree remove` on a large `node_modules` blew the 60s REST timeout, which is why removal is a rename.
- `ForceDestroy` exists (`worktree remove --force` plus `os.RemoveAll`) as a backstop, not the default.
- Uncommitted work can be captured without touching the real index: temp `GIT_INDEX_FILE`, commit object at `refs/ao/preserved/<session-id>` (`StashUncommitted`). Re-apply uses `cherry-pick --no-commit` and can leave conflict markers (`ApplyPreserved`; restore logs and relaunches with markers in place).
- Branch delete is not "on kill". Terminate-on-PR-merge is a user flag. Session branch retention is the default.

## 4. Readiness, turn completion, blocked

Hooks are authoritative. Screen is a stale-activity backstop.

**VERIFIED-IN-SOURCE** Claude (`claudecode/hooks.go`, `activity.go`): workspace `.claude/settings.local.json` runs `ao hooks claude-code <event>`, which POSTs normalized activity. Installed events: SessionStart (matcher `startup|resume|clear|compact|fork`), UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, PermissionRequest, Stop, Notification, SubagentStop, SessionEnd. Mapping:

- `user-prompt-submit`, `pre-tool-use`, `post-tool-use`, `post-tool-use-failure` → `active`
- `permission-request` and Notification `permission_prompt` → `blocked` (payload names the tool)
- `stop`, Notification `idle_prompt` and `agent_completed` → `idle` (alive, not exited)
- Notification `agent_needs_input` → `waiting_input` (no tool id, so it must not stick as `blocked`)
- SessionEnd `clear`/`resume` → no activity change; other reasons → `exited`
- SessionStart is metadata only and is **not** an activity signal (`DeriveActivityState` default)

Lifecycle clears sticky `blocked` only when the **post of the same tool** arrives. The comment says a naive "any PostToolUse → active" mapping was reverted because parallel subagent traffic cleared a live permission dialog (PR #5).

**VERIFIED-IN-SOURCE** Codex (`codex/activity.go`): `user-prompt-submit` → active, `stop` → idle, `permission-request` → `waiting_input`, not `blocked`. Reason in source: Codex installs no pre/post-tool hooks, so `blocked` could never be cleared before the turn ends. `waiting_input` still suppresses automated nudges.

**VERIFIED-IN-SOURCE** shared adapters (`activitystate/activitystate.go`): `session-start` and `user-prompt-submit` → active, `stop` → idle, `permission-request` → `waiting_input`, same "cannot clear blocked" reason.

**VERIFIED-IN-SOURCE** `ApplyActivitySignal` (`lifecycle/manager.go`): event `subagent-stop` returns immediately and does not change activity. Usage for subagents is collected separately. This is stronger than "still working": the parent state is untouched.

**Startup ready (VERIFIED-IN-SOURCE `message_delivery.go`, `sessionguard/guard.go`):**

- `WaitForMessageDeliveryReady` polls every 150ms. For TUI adapters with a terminal detector, ready means the captured pane **authoritatively** shows idle, then stays idle for 750ms. Hook state `idle` alone is not enough when a detector exists. Adapters that declare `FirstSignalProvesInputReady` are not ready until `firstSignalAt` is set. Hookless adapters wait until idle, then fall back after 5s.
- The guard's `SuppressedStartupPending` refuses any pane write before the first hook on those adapters. Reason in source: Cursor's MCP approval dialog leaves the row idle with no signal, and a paste would hit the dialog.

**Screen (VERIFIED-IN-SOURCE `observe/activity/observer.go`, `claudecode/terminal_activity.go`):** every 30s, for sessions whose hook activity has been `active` with no update for 2 minutes, capture 40 lines and let the adapter interpret them. Claude returns idle only when the rendered composer is positively idle (no spinner, no decision frame), or `waiting_input` on the literal "login expired" / "run /login" pair. Claude does **not** implement continuous detection: hooks stay authoritative while they flow, and the screen must not promote or demote sticky states by itself. `tmux capture-pane -p -S -<n>` (`adapters/runtime/tmux/commands.go`); styled capture (`-e`) exists for SGR-sensitive callers.

**No-signal (VERIFIED-IN-SOURCE `service/session/status.go`):** 90s after spawn/restore with zero hooks, display status becomes `no_signal` instead of a confident idle. Codex note in that comment: its SessionStart hook fires but carries no activity; the first activity hook is UserPromptSubmit.

## 5. Prompts, follow-ups, reading output

**Initial prompt:** usually argv (`PromptDeliveryInCommand`). Standing worker/orchestrator rules go in the system prompt, not in the user brief (`session_manager/prompt.go`): report via `ao report`, do not use the runtime's subagent tools, spawn AO workers through `ao spawn`, do not merge unless asked. Claude gets that file via `--append-system-prompt-file`. The user brief is the positional prompt.

**Follow-ups (VERIFIED-IN-SOURCE `Manager.Send` / `send`):**

- Chat: native `sendChat`. No keystrokes.
- TUI: guarded write, then tmux `send-keys -l` in chunks (default 16KB) and a separate Enter after `defaultEnterDelay` = 300ms (`adapters/runtime/tmux/tmux.go`). Same 300ms constant in conpty (`ptyInputEnterDelay`). Empty message is Enter alone (nudge). Once the paste is in the pane, the delay and Enter are detached from caller cancellation so a cancel cannot leave a draft that a retry would double-paste.
- Guard re-reads the session immediately before the write (`sessionguard/guard.go`) under an input lease held until the write returns. Refusals: not found, terminated, agent exited (the pane may now be a shell; a prompt would execute as shell input), `blocked` for ordinary deliver, startup pending, input gated by an in-progress switch/restore/kill. `waiting_input` does **not** refuse a user send. Nudges refuse both `waiting_input` and `blocked`.
- After a successful paste, `confirmActive` polls activity every 300ms for up to 2s, up to 3 Enters total, **only** for harnesses that emit both a prompt-submit signal and a clearable `blocked` signal (comment: Claude and hook-delegators). If the session becomes `blocked`, it stops and does not press Enter. If the budget runs out it **logs and returns success**. Source comment: "AO has no delivery ack… Confirmation never fails the send."
- `SendSemantic` is the strict path used for report injection. It wraps the body as `<ao-report-delivery id="…">`, writes it, and waits up to 10s for a coordination checkpoint whose turn id equals that delivery id (`ConversationCheckpointTurnID`). Pane-write success is not acceptance. Timeout returns an error (`prompt submission was not observed`). Chat uses the provider's accepted-turn boundary instead.

**Reading output:** operators attach to the terminal websocket or call `GetOutput` (capture-pane window). There is no monotonic rendered-text cursor. Hook payloads carry `last_assistant_message` into `LatestAssistantUpdate` on Stop, with guards so an internal coordination turn does not replace the user-facing update. Native transcript path is stored but is not the activity channel. Usage ingestion tails Claude JSONL, Codex rollout, and Kimi wire separately (`domain/usage.go`) and is explicitly not the lifecycle signal (`activity.go`: "not inferred from transcript/JSONL").

## 6. Result handoff and parent/child

**VERIFIED-IN-SOURCE** `ao report` (`prompt.go`, `domain/report.go`, `service/report/coordinator.go`):

- Worker runs `ao report --checkpoint|--needs-input|--stuck|--done --note …` plus `--artifact`, `--pr-created`, `--pr-reviewed`. `AO_SESSION_ID` selects the worker. Notes are capped at 1000 characters (`MaxReportTextCharacters`).
- A coordinator batches pending reports per project and submits them to the **newest non-terminated, non-exited orchestrator session in that project**, not to a stored parent.
- Delivery is semantic: `SendSemantic` (exact id ack) or, for `stuck`, an interrupt of the orchestrator's current turn, coalesced to one interrupt per worker per 3 minutes (`ReportInterruptWindow`). Claim token fences the batch. Submit failure releases the claim; success acknowledges it. No orchestrator → error `project has no active orchestrator`.
- Reports do not flip the worker to a terminal session state. The orchestrator is prompted to read them and decide.

Parent/child in the 018 sense does not exist. Lineage is "whoever called `ao spawn` inherited permissions" plus "whichever orchestrator is the latest live one receives the report batch." Multiple orchestrators in one project race on `CreatedAt`, newest wins (`activeOrchestrator`).

Worker-to-orchestrator chat is `ao send --session <orchestrator-id>`, which is the same guarded send, not a side channel (`workerOrchestratorPrompt`).

## 7. Review and merge

**VERIFIED-IN-SOURCE:** finish is a GitHub (or configured SCM) pull request, not a local `git merge` into the source checkout.

- Review engine (`backend/internal/review/review.go`): a reviewer harness runs against the worker worktree; runs are keyed by session, PR URL, and target SHA; stale running runs are superseded. Headless reviewer sessions (`AO_REVIEW_SESSION_ID`) may have the Claude PermissionRequest hook answer allow/deny because nobody can click the dialog (`claudecode/hooks.go` comment, issue #4810).
- SCM observer stores PR, CI, review, and comment facts. Lifecycle reactions (`lifecycle/reactions.go`) may paste CI failures and review comments back to the owning worker when `autoInjectCI` / `autoInjectReview` are set. Those nudges are suppressed while the agent `NeedsInput`, except a merge-conflict nudge, which still refuses a live permission dialog and an unproven `waiting_input` composer. A PR stacked on an open parent is not told to rebase (conflicts against the parent are expected). Each nudge dedups on a signature and has its own attempt budget.
- Human merge: `POST /prs/{id}/merge` → `MergePullRequest` (`adapters/scm/github/merge_action.go`). Only squash. GitHub `sha` is a compare-and-swap on the reviewed head. 409 → head changed, 405/422 → not mergeable. No local conflict resolution and no auto-merge. Orchestrator system prompt: "Do not merge unless explicitly asked."
- Kanban `ready` / `needs_review` / `validating` is derived from those PR facts (`pkg/contract/kanban.go`). Auto-inject flags credit "Fixing CI" only while activity is `active`.
- `TerminateOnPRMerge` can tear the session down after the PR set merges. That is policy, not an automatic local branch delete.

Local dirty-target / abort-and-leave-source-clean merge, which 018 specifies, is not this system's finish path.

## 8. Concurrency, cost, observability

**Concurrency:** no session cap turned up under `backend/internal/config` or spawn. Parallelism is "as many tmux sessions as the machine will run." The only small cap found is cosmetic: delegated-task title refinement runs at most 4 background calls and drops the rest (`delegation.go`, `delegatedTaskTitleConcurrency = 4`). Workspace creation for one project is serialized by `acquireWorkspaceGate`. Pane writes take an input lease so kill/switch waits for an in-flight paste.

**Cost (VERIFIED-IN-SOURCE `domain/usage.go`, `domain/conversation.go`, daemon wiring):** a usage pipeline tails native artifacts (Claude main and subagent JSONL, Codex rollout, Kimi wire) into usage events with nil-vs-zero token counters and an optional monetary `Cost`. `ConversationUsage` is latest-wins context position, not a budget. `UsageErrorCodexSourceBudgetExceeded` is a parser failure when a Codex artifact is too large to ingest (`collector_budget_test.go` names it `codex_source_budget_exceeded`), not a spend kill switch. No max-USD or max-token enforcement showed up in backend config. **INFERRED:** AO observes spend for some harnesses and does not stop a session for cost.

**Observability:** SQLite facts, CDC → SSE, terminal websocket, Kanban derived status, `no_signal`, hook-derived `firstSignalAt`, cleanup disposition, report delivery state, review-run status. Daemon logs are slog text on stderr. Desktop is the primary UI; CLI is REST. Mobile is an authenticated LAN/tunnel surface (`CONTEXT.md`, ADR 0001/0004) and is out of scope for 018.

## 9. Better than 018, and what not to copy

Better, with the path:

- **Blocked is a permission dialog, and only if some later event can clear that exact dialog.** Claude stores the blocking tool and lets only that tool's PostToolUse lift `blocked` (`claudecode/activity.go`, lifecycle comment in the same file). Parallel subagent tool traffic must not. 018 FR-006 maps every `PostToolUse` to `working`, which is the mapping AO reverted.
- **`SubagentStop` is ignored**, not turned into `working` (`lifecycle/manager.go`). 018 FR-006 says `SubagentStop→working`, which marks an idle parent busy when a provider subagent ends.
- **Silence is not idle.** No first hook ⇒ startup-pending (writes refused) and, after 90s, display `no_signal` (`sessionguard/guard.go`, `service/session/status.go`). Claude SessionStart does not mean ready (`claudecode/activity.go`).
- **Strict delivery exists, but only on the semantic path.** `SendSemantic` fails at 10s unless the hook echoes the delivery id (`manager.go`). Ordinary `Send` does not.
- **Input lease plus re-read immediately before write**, and cancellation must not strand a partial paste (`sessionguard/guard.go`, `tmux.go`).
- **Idempotent create** via `clientRequestId` + body hash, and incomplete launches are not replayed as success (`session.go`, `Spawn`).
- **Dirt is judged after the directory is renamed aside, and a dirty tree is renamed back** (`gitworktree/discard.go`). Check-then-delete races stray writers.
- **Seed ref and comparison ref are different**, and the stored diff SHA is a merge-base against the default ref (`workspace.go` `worktreeRefs`, `resolveSpawnDiffBase`). A branch seeded from `origin/<feature>` still diffs against the default.
- **Fetch of the default branch is best-effort, one 5s budget, continue on local refs** (`refreshDefaultBranchesBestEffort`). Failure does not abort spawn.
- **Standing worker preamble**, separate from the user brief, tells the agent to `ao report` and forbids runtime subagents (`prompt.go`). The brief alone is not the protocol.
- **Report outbox** with claim, ack, idempotency key, settlement window, and interrupt coalescing (`report.go`, `coordinator.go`). It does not pretend a report is the session lifecycle.
- **Exited-agent pane is treated as a shell** and writes are refused (`sessionguard` `SuppressedExited`).
- **Mass-death breaker** on the reaper (`reaper.go`) so one bad probe pass cannot archive every session.
- **Preserve uncommitted work in a side ref without mutating the real index** (`StashUncommitted`). Useful later; not required for 018's explicit keep-the-worktree rule.

Do not copy:

- **Ordinary send returns success when tmux exits 0**, and the activity confirmation is best-effort (`Send` comment: confirmation never fails the send). 018's explicit stall error is the stricter contract. Copy `SendSemantic`, not `Send`.
- **GitHub squash merge as the only finish**, with CI/review auto-paste and a Kanban of derived PR states. 018 is a local multiplexer: diff against a recorded base, merge or discard in git, human gate on dirt and conflicts. AO never locally merges into the source checkout (`merge_action.go` is the GitHub API).
- **Derived status instead of a durable task lifecycle.** AO can do that because the session row and the PR rows are the facts. 018's callers are other agents and need an immutable terminal state, a stored result, and restart-to-failed. Do not replace the task with a Kanban derivation.
- **Project-singleton report routing and no stored parent.** Newest orchestrator wins; parent id is permission inheritance only. 018's `parent_pane_id` / `root_pane_id` is the right model when several panes can orchestrate.
- **Reports that do not finish the task.** Right for AO's long-lived session. Wrong for 018, where `signaltty report` is the terminal handoff. Keep terminal-on-report. Steal the outbox only for the deferred injection PR.
- **`permission-request` → `waiting_input` on Codex and most adapters**, which exists so a later user send is not refused forever. 018 refuses prompts while blocked and answers permissions on `decision.answer`. Collapsing the two states would let a prompt hit a permission dialog. Do copy their rule about when `blocked` is even legal (you must be able to clear it).
- **Surviving the daemon by leaving tmux and chat hosts running.** Signaltty's PTY dies with the server (ADR-0020, docs/09). Adopting a dead pane as still `working` is the limbo 018 already rejected. Also do not copy "sticky activity prevents termination" (`runtimeClearlyDead`).
- **Unlimited sessions.** AO has no worker cap. 018's default 4 stays.
- **Cost enforcement.** AO records tokens for three artifact shapes and does not stop work. Nothing here overturns the 018 deferral. If cost is added later, ingest native usage records; do not parse transcripts for lifecycle.
- **Screen as a continuous status source, trust-prompt scraping, auto-yes, or English stderr matching.** AO's screen path is a 2-minute stale-active reconciler with a positive idle-composer test. PreLaunch writes Claude's trust config so the trust dialog never appears (`claudecode.go` `PreLaunch`); that is harness-specific setup, not a scraper, and it is outside 018's agent-agnostic scope.
- **Speculative worktrees for a New Task dialog, title-refinement side calls, mobile LAN, and the 33-harness matrix.** Product surface, not the 018 contract.
- **Moving diff base.** AO recomputes merge-base against the default ref at spawn, so the baseline can be `origin/main` rather than the commit the branch was cut from. 018's recorded base SHA is the stable review baseline. Keep it. The useful piece is only "prefer a configured default over the source checkout's current branch," which 018 already records as `target_branch` at start.

## Deltas for 018

1. **FR-006 / lifecycle: a `PostToolUse` must not clear `blocked` unless it is the completion of the tool that raised the permission.** Record the blocking `tool_use_id` (or equivalent) on the pane. Parallel subagent tool traffic stays on the parent as still-working and must not demote `blocked`. Source: `claudecode/activity.go` (the reverted naive mapping). Without this, 018's "PostToolUse → working" lets a stray tool event open the submit gate while a permission dialog is up.

2. **FR-006: ignore `SubagentStop` for parent lifecycle.** Do not transition the parent to `working`. Source: `lifecycle/manager.go` (`subagent-stop` returns nil). The user-story line "SubagentStop → still working" is the right outcome; the FR arrow `SubagentStop→working` is not, because an idle parent would become working.

3. **Ready is not SessionStart, and silence is not idle.** Production rule: no activity-bearing hook yet ⇒ not ready (`AGENT_NOT_READY` / startup-pending), writes refused. Claude `SessionStart` carries no activity (`claudecode/activity.go`). After a grace with still no signal, surface `no_signal` rather than idle (AO uses 90s, `service/session/status.go`). Fix the e2e sentence that says `SessionStart` hook → idle; the fake may emit an explicit ready event, and the spec should name that event. Startup settle (AO: authoritative idle composer for 750ms, or first signal) is the model for the background ready-wait.

4. **Keep the explicit stall failure. Tighten it to the semantic ack, and do not copy AO's ordinary `Send`.** `Send` returns success when paste+Enter exits 0 and only best-effort nudges (`manager.go` `confirmActive`, budget 3×2s, never fails the send). `SendSemantic` waits 10s for the hook to echo the delivery id and returns an error if it does not. 018 should correlate the activity gate with the submitted prompt (delivery id or prompt checkpoint), not with any newer working/blocked event, and must still fail the call when the budget expires.

5. **`task.start` takes an optional client request id.** Same id and same body returns the existing task after the create has committed; same id and a different body is a conflict; an incomplete prior start is an error, not a second worktree. Source: `SessionRecord.ClientRequestID` / `ClientRequestCommitted`, `Spawn`, `delegation.go`. Orchestrator retries will double-spawn without this.

6. **Submit the objective verbatim as the user prompt, and inject a fixed worker preamble beside it.** AO's user brief is not the protocol; `workerSystemPrompt` (`prompt.go`) tells the agent to `ao report` with the status/note/artifact flags and forbids runtime subagents. If 018 submits only the objective, real workers will not call `signaltty report`, and `turn_ended_without_report` becomes the normal path. Preamble: this worktree only, report schema, no provider subagents, do not merge yourself. Do not fold the preamble into the stored objective.

7. **Discard/merge dirt check races.** When removing a worktree, rename it aside, run porcelain at the new path, and rename it back if dirty or if the probe is inconclusive. Do not check and then delete. Source: `gitworktree/discard.go` `discardWorktree`. Unlink can be background after git has dropped the registration. This does not change the refuse-dirty rule.

8. **Screen fallback sentence in FR-006.** Fallback may run only after hook activity has gone stale (AO: 2 minutes, poll 30s), may only promote to idle when the captured pane positively shows an idle composer, and must not demote sticky blocked/waiting and must not run while hooks are still flowing. Source: `observe/activity/observer.go`, `claudecode/terminal_activity.go`.

9. **Per-pane write lease across paste and the delayed Enter.** Cancel, discard, and a second submit wait for the lease or are refused. A cancel after the paste has landed must not skip the Enter in a way that leaves a draft a retry will paste again (AO detaches the Enter from caller cancel once chunks are accepted, `tmux.go`). 018's "no torn writes" edge case needs this mechanism.

10. **No change** to local merge-or-discard, recorded base SHA as the diff baseline, concurrency cap 4, restart-to-failed when the pane is dead, terminal-on-report, stored parent lineage, or the cost/Kanban/MCP deferrals. AO's GitHub squash finish, derived Kanban, parentless report routing, unlimited workers, best-effort send success, sticky-activity-blocks-death, and "reports do not end the session" are the wrong model for this contract. The report outbox (`pending/claimed/acknowledged`, delivery id wrapper) stays the shape of the deferred injection PR; 018 already says delivery states wrap the result, so that is not a new field in this PR.
