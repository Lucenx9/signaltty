# Feature specification: Task orchestration for in-pane agents

**Feature Branch**: `orch/tasks`
**Created**: 2026-10-04
**Status**: Draft
**Input**: One complete PR delivering an orchestrator "built to best practice": an agent
running in a signaltty pane can spawn worker agents each in its own git worktree from a
recorded base commit, deliver prompts reliably, know when workers are ready / working /
blocked / done via hooks (screen only as fallback), read rendered output incrementally,
receive structured results, see who needs the user, diff each task against its base, and
finish each task by explicit merge or discard with safe cleanup — surviving server restart.

Evidence base: `research-codebase.md` (what signaltty has today),
`research-external.md` (t3code, cmux, herdr, workmux, Conductor, vibe-kanban,
claude-squad, source-verified), `research-practices.md` (2026 best practices).
Every load-bearing choice below cites its source there.

## User scenarios and testing

### User story 1: Spawn worker tasks in isolated worktrees (P1)

As an orchestrator agent in a pane, I can start worker tasks so each worker runs in
its own worktree + branch cut from a recorded base commit, with parent/child lineage
and a label, without disturbing my checkout or other workers.

**Why this priority**: Isolation is the precondition for everything else: no shared
checkouts (research-practices §7), no dirty-tree bases, no collisions.

**Independent Test**: Start a task in a temp repo, assert the worktree/branch exist,
the base SHA is recorded, the pane runs there, and lineage/labels are stored.

**Acceptance scenarios**:

1. **Given** a clean repo, **When** I start a task, **Then** cap → base →
   worktree → spawn run synchronously (fast, bounded) and the call returns
   `{task, pane}` with the task `pending`; the ready-wait + gated submit run
   server-side in the background and move the task to `working` (submit
   accepted) or `failed` with `{stage: ready_timeout|submit_refused|
   activity_gate, …}` evidence. The caller observes via `task.wait --until
   working` or events.
2. **Given** a `parent_pane_id` and label, **When** the worker spawns, **Then** the
   pane records parent/root/relationship/label, `$SIGNALTTY_PANE` still names the
   worker pane itself, and `$SIGNALTTY_PARENT_PANE` / `$SIGNALTTY_TASK` name the
   lineage.
3. **Given** more running tasks than the configured cap, **When** I start another,
   **Then** the start is refused with an explicit rate-limit error and nothing is
   created.
4. **Given** a base ref I name, **When** the task starts, **Then** the recorded base
   SHA resolves that ref (optionally after fetch); the default base is local `HEAD`
   with no network.

### User story 2: Submit prompts reliably and know worker state (P1)

As an orchestrator, I can deliver a prompt to a worker exactly once, have the
delivery refused before any write when the worker is busy/blocked/not-ready, and
get an explicit stall signal when the prompt does not visibly land — with
ready/working/blocked/done coming from hooks, not screen scraping.

**Why this priority**: Lost or double-delivered prompts corrupt turns silently;
herdr proves the submit gate is the fix (research-external rec. 1).

**Independent Test**: Submit to idle/done/working/blocked panes and assert
accept/refuse/stall outcomes; drive transitions with synthetic hook events.

**Acceptance scenarios**:

1. **Given** an idle or turn-done worker, **When** I submit text, **Then** it is
   pasted bracketed and submitted with a separate Enter, and a newer
   working/blocked transition is observed within the stall budget.
2. **Given** a working or blocked worker, **When** I submit, **Then** the submit is
   refused before any byte is written, with a busy/not-ready error telling me which.
3. **Given** a submit whose prompt never produces activity, **When** the stall budget
   expires, **Then** I get an explicit timeout naming the activity-gate stage —
   never a silent success.
4. **Given** hook events, **When** the worker lifecycle moves, **Then** submit
   readiness follows hooks (`Stop`→done, `PermissionRequest`→blocked,
   `SubagentStop`→still working); screen/OSC stay fallback only.

### User story 3: Read worker output incrementally (P1)

As an orchestrator, I can read a worker's rendered terminal output as text that
reflects what the worker actually drew (no TUI garbling), keep scrollback, and
poll for "only what's new" via a cursor — with an explicit signal when the range
I asked for has been evicted.

**Why this priority**: `tail`-mode garbling makes output unreadable today
(research-codebase §1); nobody else has the cursor, so this is the differentiator
(research-external rec. 4).

**Independent Test**: Feed TUI repaint bytes, read incrementally twice, force
scrollback eviction, and assert rendered text, delta-only second read, and the
dropped-range signal.

**Acceptance scenarios**:

1. **Given** a worker running a full-screen TUI, **When** I read its output,
   **Then** I get rendered lines (cursor moves, overwrites, repaints resolved),
   not concatenated fragments.
2. **Given** a cursor from a previous read, **When** I read again, **Then** I get
   only newer rendered lines plus the next cursor; an unchanged pane returns empty
   text with an unchanged head.
3. **Given** a cursor older than retained scrollback, **When** I read, **Then** the
   reply says the range was dropped and returns what is still available.
4. **Given** existing `screen`/`tail` callers, **When** they read, **Then** their
   shape is unchanged; `tail` additionally becomes cursor-correct.

### User story 4: Collect structured results and see who needs the user (P1)

As an orchestrator, I can have each worker report a structured result (status,
summary, artifacts, evidence) that is stored on the task and emitted as an event,
wait for tasks to finish, and list every pane needing the user, ranked.

**Why this priority**: Results are the handoff (A2A artifacts); the ranked
needs-user list is the attention loop (product directive 1).

**Independent Test**: Report results from worker panes, wait on one task and on a
whole context, block one worker on a permission, answer it, and assert listing
rank and result storage.

**Acceptance scenarios**:

1. **Given** a running task, **When** its worker reports a result, **Then** the
   result is stored on the task, the task reaches the matching terminal state,
   and a result event is emitted.
1b. **Given** a `working` task whose worker pane reaches lifecycle `done`
   (Stop hook) with no report received, **When** the turn ends, **Then** the
   task moves to `input_required` with evidence `{reason:
   "turn_ended_without_report", last_message?}` (A2A interrupted: the
   requester must act — follow up via `pane.submit`, which moves the task back
   to `working`, or cancel/discard); it never hangs waiting for a report that
   will never come.
2. **Given** several tasks in one context, **When** I wait on the context, **Then**
   the wait ends when every task in it is terminal, or times out explicitly.
3. **Given** workers blocked/done/working, **When** I list who needs the user,
   **Then** I get all attention-raised panes ranked by documented severity, then
   recency — not just the single top item.
4. **Given** a worker blocked on a permission, **When** it is answered through the
   existing decision channel, **Then** the worker resumes and the listing updates.

### User story 5: Review diffs and finish explicitly (P1)

As an orchestrator (or user), I can diff each task against its own recorded base,
then finish it by explicit merge or discard — with dirty-tree, conflict, and
branch-safety rules — and get cleanup errors reported separately from the merge
outcome.

**Why this priority**: Diff-then-merge-or-discard is the review gate every mature
tool converges on (research-external rec. 6, research-practices P9).

**Independent Test**: Complete tasks with conflicting and clean changes; assert
diff-vs-base content, merge success/failure semantics, target-clean-on-conflict,
branch retention, and worktree cleanup.

**Acceptance scenarios**:

1. **Given** a finished task, **When** I diff it, **Then** I see worktree-vs-base-SHA
   changes (tracked + untracked), computed without mutating the index.
2. **Given** a completed task with a clean target, **When** I finish with merge,
   **Then** the branch merges, the worktree is removed, and the task records the
   merge. A task-created branch is deleted by default; `--keep-branch`
   retains it. A pre-existing branch is never deleted.
3. **Given** a merge conflict, **When** I finish with merge, **Then** the target
   is left clean (merge aborted), the conflict stays in the source worktree with
   the conflicted files named, and the task records the failure.
4. **Given** a dirty source I am about to delete, or a dirty target, **When** I
   finish with merge, **Then** the finish is refused unless I explicitly ignore
   dirt; staged work is never auto-committed.
5. **Given** any task, **When** I finish with discard, **Then** the worker stops,
   only that task's worktree is removed, and the branch is kept unless explicitly
   deleted (and never when pre-existing).
6. **Given** a merge that lands but whose cleanup fails, **When** I read the
   result, **Then** the merge still reports success with the cleanup error
   attached separately.

### User story 6: Survive crash and restart without limbo (P1)

As an orchestrator, my tasks persist across server restart: completed work and
results are intact, and anything whose worker died becomes failed-with-evidence —
never stuck, never silently restarted.

**Why this priority**: The user requirement names restart survival; processes
cannot survive (docs/09 honest rule), so state must be explicit.

**Independent Test**: Restart the server mid-run with completed and running tasks;
assert completed tasks keep results, running tasks with dead panes become failed
with evidence, and checkouts are preserved for resume-as-new-task.

**Acceptance scenarios**:

1. **Given** completed tasks with results, **When** the server restarts, **Then**
   tasks, results, and dispositions are intact and diffs still work.
2. **Given** a running task whose pane died (crash or restart), **When** recovery
   runs, **Then** the task becomes failed with evidence (reason, exit code when
   known, last lifecycle) and its worktree + branch are preserved.
3. **Given** a worker pane exit, **When** its task is not terminal, **Then** the
   task transitions to failed with evidence exactly once.
4. **Given** old snapshots without tasks, **When** the server loads them, **Then**
   they load cleanly (tasks default empty) and nothing is discarded as corrupt.

### User story 7: See tasks in the GUI (P2)

As a user, I can see which panes belong to tasks and what state those tasks are
in, inside the existing workspace UI — without learning a new view.

**Why this priority**: Visibility honors the attention-first shell; a kanban
board is a separate feature (deferred, with reasons below).

**Independent Test**: Start a task, open the workspace, and assert the worker
pane row shows the task label and state in light and dark schemes.

**Acceptance scenarios**:

1. **Given** a pane owned by a task, **When** I look at the workspace sidebar,
   **Then** the pane row shows the task label and current task state.
2. **Given** a task state change, **When** the event arrives, **Then** the row
   updates without rebuilding the terminal.

### Edge cases

- Empty text submit; submit to exited/unknown/failed panes; submit racing a turn
  end; two orchestrators submitting concurrently (second sees busy or a newer
  baseline — no torn writes; callers serialize, as with `wait.after`).
- Worker emits `Stop` before the submit gate attaches (fast completion) — the
  pre-submit baseline still matches it, as with `wait.after` (research-codebase §3).
- Worker in alt-screen: reads return the current grid; no alt-screen scrollback
  exists (engine limit, documented, not silent).
- Report for an unknown task, for a terminal task, or from a foreign pane.
- `task.wait` on an empty context (succeeds immediately: nothing outstanding) vs
  unknown task id (explicit not-found).
- Base ref unresolvable, empty repo (no HEAD), worktree path already registered,
  branch name collision with an existing branch (adopt as pre-existing, never
  delete).
- Concurrent `task.start` for the same path (per-path lock serializes).
- Merge into a target whose HEAD moved since start (allowed; conflict rules still
  apply — HEAD movement is normal, dirt is what we refuse).
- Restart between merge landing and cleanup (cleanup retried or reported on next
  finish/status read — merge is never un-landed).
- Restart while a task is still `pending` (background ready-wait died with the
  server) → task `failed` with evidence `{stage: "restart"}` on recovery.
- `pane.submit` to a worker pane whose task is still `pending` (background submit
  in flight) → refused `AGENT_NOT_READY` before any write.
- Legacy snapshots, corrupt snapshots (existing backup-and-start-empty rule stands).
- Attention listing with hundreds of panes (bounded default limit).

## Requirements

### Foundations

- **FR-001**: `tail`-mode reads MUST return cursor-aware rendered text (overwrites,
  repaints, carriage returns resolved), derived from the terminal grid — not from
  raw byte-stream splitting.
- **FR-002**: The server MUST retain at least 5000 rendered scrollback lines per
  pane at runtime and expose an incremental read with a monotonic cursor plus an
  explicit dropped-range signal.
- **FR-003**: Prompt submission MUST paste text bracketed and send Enter (`\r`) as
  a separate delayed write (default 300 ms, caller-overridable).
- **FR-004**: Submission MUST refuse before any write when the pane is busy
  (working/blocked), not ready (unknown/failed/not started), or exited, with
  distinct error codes for busy vs not-ready vs exited.
- **FR-005**: Submission MUST confirm the prompt landed via a newer working-or-
  blocked transition within a stall budget (default 5 s) measured against a
  pre-submit baseline; expiry returns an explicit timeout naming the
  activity-gate stage.
- **FR-006**: Readiness and completion signals MUST come from hooks first
  (`UserPromptSubmit`/`PostToolUse`→working, `PermissionRequest`→blocked,
  `Stop`→done, `SubagentStop`→working); screen/OSC stay fallback only.
- **FR-007**: `pane.spawn` MUST accept optional parent pane, label, and
  relationship (`fork`|`subagent`); the server derives the root pane, preserves
  `$SIGNALTTY_PANE` = own pane id, and injects `$SIGNALTTY_PARENT_PANE` and
  `$SIGNALTTY_TASK` when applicable.
- **FR-008**: The server MUST list all panes currently raising attention, ranked
  by the documented severity order then recency, with a bounded default limit.

### Task core

- **FR-009**: A Task entity MUST persist in the snapshot (serde-defaulted) with an
  A2A-aligned lifecycle (`pending → working ⇄ input_required → completed | failed
  | canceled | rejected`); terminal states are immutable and refinements are new
  tasks in the same context.
- **FR-010**: Every task MUST carry a contract: objective (required),
  constraints, acceptance criteria, and expected output format; `task.start`
  refuses an empty objective.
- **FR-011**: `task.start` MUST run cap → base → worktree → spawn synchronously
  (fast, bounded) and return `{task, pane}` with the task `pending`; the
  ready-wait + gated submit MUST run server-side in the background (surviving
  client disconnect, cancelled cleanly by `task.cancel`/discard/shutdown),
  moving the task to `working` on submit-accept or `failed` with `{stage:
  ready_timeout|submit_refused, …}` evidence. An unconfirmed activity gate
  parks the task at `input_required` with `submit_unconfirmed` evidence;
  a later worker turn resumes it. `task.start` MUST
  record the resolved base SHA, the `target_branch` (source repo's
  checked-out branch; detached HEAD leaves it unset), create worktree +
  branch under a per-path lock, and spawn the worker pane there; default
  worktree path is `$XDG_DATA_HOME/signaltty/worktrees/<repo-dirname>-
  <short-hash-of-repo-path>/<sanitized-branch>` (fallback `~/.local/share`),
  explicit `path` wins. Failures after the worktree exists leave a failed task
  with evidence (`stage`), never limbo; validation failures before it (bad
  params, unresolvable base ref, unsendable contract) return a synchronous
  error and create no task.
- **FR-012**: `task.wait` MUST wait for one task or for all tasks in a context to
  reach the requested states (default `until: settled` = any terminal state OR
  `input_required`), with explicit timeout.
- **FR-013**: `task.get` / `task.list` MUST expose tasks (list filterable by
  context and state).
- **FR-014**: `task.diff` MUST compare worktree-vs-recorded-base (tracked and
  untracked) with no index mutation (`git add -N` forbidden as a read side effect).
- **FR-015**: `task.finish --merge` MUST require a completed task and a clean
  target, refuse dirt per the workmux rules, abort conflicts leaving the target
  clean, delete task-created branches by default unless `--keep-branch`
  is set, preserve pre-existing branches, and report
  cleanup failures separately from merge success. The merge target defaults to
  the recorded `target_branch` (FR-024).
- **FR-016**: `task.finish --discard` MUST stop the worker and remove only that
  task's worktree, keeping the branch unless explicitly deleted (never when
  pre-existing). A second finish retries cleanup while the recorded worktree
  exists and is refused once that path is gone.
- **FR-017**: Worker results (status, summary, artifacts, evidence) MUST be stored
  on the task via `signaltty report` and emitted as a task event.
- **FR-018**: Task transitions MUST emit `task.*` events into the existing
  journaled event stream; subscribe MUST support the `task.*` glob and
  task-scoped replay.
- **FR-019**: Pane death or server restart MUST move non-terminal tasks with dead
  panes to failed with evidence, preserving worktree + branch; completed tasks
  and results MUST survive restart intact.
- **FR-020**: Concurrent non-terminal tasks MUST be capped (configurable, default
  4); over-cap starts are refused before creating anything.
- **FR-021**: Every task operation MUST have CLI parity with `--json` passthrough.
- **FR-022**: The GUI MUST show task label + state on worker pane rows and update
  on task events without rebuilding terminals; no new views in this PR.
- **FR-023**: A worker pane reaching lifecycle `done` while its task is `working`
  with no report received MUST move the task to `input_required` with evidence
  `{reason: "turn_ended_without_report", last_message?}`; an accepted
  follow-up submit on that worker pane moves it back to `working` (A2A
  interrupted state — VERIFIED
  https://a2a-protocol.org/dev/topics/life-of-a-task/).
- **FR-024**: `task.start` MUST record `target_branch` = the source repo's
  currently checked-out branch (detached HEAD → unset, explicit `target_ref`
  required at finish). `task.finish --merge` MUST default to the recorded
  `target_branch` and refuse with `BAD_PARAMS` + `details: {expected, actual}`
  unless that branch (or an explicit `target_ref`, same rule) is currently
  checked out in the source repo — never merge into whatever branch the user
  switched to.

## Deterministic end-to-end acceptance scenario

This is the quality bar and the first test written
(`crates/signaltty-server/tests/`, `signaltty-testkit`, fake agent = small
script + synthetic `hook-event` calls, no real LLM):

1. Create a temp git repo with one commit; start an orchestrator pane (plain shell).
2. Orchestrator starts 3 tasks sharing one context, each with a contract
   (objective + acceptance criteria), in that repo — the 3 starts are issued
   back-to-back (async: each returns at `pending`), then the orchestrator
   waits for all three to reach `working` (background ready + submit).
3. Each worker pane becomes ready (`SessionStart` hook → idle); objectives are
   submitted (one submit each, no stalls).
4. Workers turn working (`UserPromptSubmit`), then: worker A reports a completed
   result; worker B raises `PermissionRequest`, appears in the needs-user
   listing, is answered via the native waiting hook fixture, then reports
   completed; worker C ends its turn WITHOUT reporting → its task moves to
   `input_required` (`turn_ended_without_report`); the orchestrator sees it on
   the settled wait, sends a follow-up via `pane.submit` (task back to
   `working`), and worker C then reports completed.
5. Orchestrator waits on the context with the default `settled` until (all
   terminal or input_required — the wait ends at worker C's `input_required`,
   then again at all-terminal after the follow-up), reads each worker's output
   incrementally (second read returns only new lines), and lists needs-user
   (empty at the end).
6. Orchestrator diffs each task vs its recorded base (each shows only its own
   change, including an untracked file from one worker).
7. Server restarts. All 3 tasks still completed with results; diffs still work.
8. Orchestrator finishes: merge 2, discard 1. Merged branches land in the source
   repo; the discarded worktree is gone. Merged task-created branches are
   deleted unless `--keep-branch` is set; discarded branches remain unless
   explicitly deleted. The repo's pre-existing branch is untouched.
9. Failure paths (separate tests): dirty-target refusal; dirty-source refusal
   without ignore; merge conflict leaves target clean with conflicted files
   named; pane crash → task failed with evidence; over-cap start refused;
   stalled submit times out at the activity gate.

## Deferred scope (explicit, with reasons)

- **Active result injection into the parent** (cmux `agent.message` inbox,
  Claude `UserPromptSubmit additionalContext` / exit-2 wake, Codex `Stop`
  `decision:block`): deferred. `task.wait` + `task.result` + `task.get` already
  deliver results deterministically (the herdr/workmux pattern: wait on status,
  re-read); injection needs an inbox state machine plus hook-shim changes across
  providers — a second PR. The result payload shape is specified now so the
  follow-up is mechanical.
- **MCP server**: deferred. Constitution VII + docs/14 ("composable primitive
  (socket/CLI/events)") say ship the primitive first; an MCP server doubles the
  surface (new transport, tools/resources, elicitation/sampling). The IPC/CLI
  contract maps 1:1 onto future tools (mapping table in `contracts/ipc.md`).
- **Cost/token budgets**: deferred beyond the concurrency cap. No cost telemetry
  exists in signaltty (per-run cost only in headless JSON; transcript parsing is
  vendor-warned unstable); the cap + contracts bound spend coarsely. Model
  routing stays caller policy via agent argv.
- **Kanban board view**: deferred. New view + interactions at t3code visual bar
  does not fit one PR; row surfacing covers visibility now.
- **Checkpoints/rollback, turn diffs**: deferred. Checkpointing a PTY+git pair
  needs design beyond base-SHA diffs; base diffs cover review now.
- **Evaluator self-check loop** (`/goal`-style acceptance pass): deferred. The
  server is an agent-agnostic multiplexer; the orchestrator agent itself
  evaluates against contract acceptance criteria.
- **Per-agent submit profiles** (paste/Enter timing variants): deferred past the
  delay parameter. No Linux evidence for differing delays; the parameter keeps
  the seam open.
- **Alt-screen refusal for history reads** (herdr `agent_not_idle`): not copied.
  Our reads are passive and side-effect-free; refusal would buy nothing. The
  alt-screen-has-no-scrollback engine limit is documented instead.
- **Full ACP-agent exposure**: not in scope (research-practices §7 agrees).
- **Multi-server / remote workers**: not in scope; one server owns its panes.

## Success criteria

- **SC-001**: The end-to-end scenario passes deterministically (no LLM, no
  network, no sleeps over the documented stall budgets) with all 3 tasks
  merged/discarded as specified and pre-existing branches untouched.
- **SC-002**: Every exercised failure path (dirty refusals, conflict-clean
  target, crash→failed, cap refusal, submit stall) produces its specified
  explicit error; zero silent partial successes.
- **SC-003**: TUI repaint fixtures render identical text in `tail` and
  incremental reads, with zero concatenated-fragment artifacts.
- **SC-004**: A prompt submitted to a working/blocked pane writes zero bytes
  (provable by a read-before/after comparison in the regression test).
- **SC-005**: Restart with N completed + M running tasks yields N intact
  (results + dispositions) and M failed-with-evidence, with M checkouts
  preserved on disk.
- **SC-006**: `scripts/verify.sh full` green; GUI row surfacing verified with
  light + dark screenshots.

## Assumptions and clarification

- One server, one user, local-trust socket (existing model): task callers are
  authorized by socket access, as with panes today.
- Default base is local `HEAD` with no fetch; callers needing
  fetch-then-branch say so explicitly (offline-capable default beats
  Conductor's always-fetch for a local multiplexer).
- Interactive TUI agents are the workers; headless `claude -p` / `codex exec`
  workers are supported as plain panes with degraded (no-hook) completion
  detection via exit + output, per the version-fragility warning
  (research-external §B, research-practices §2.3).
- `task.start` submits the contract objective verbatim as the first prompt (via
  the server-side background step; the caller never submits the first prompt
  itself — concurrent `pane.submit` to a `pending` task's pane is refused).
- Default worktree root is `$XDG_DATA_HOME/signaltty/worktrees` (fallback
  `~/.local/share`); `git worktree list` in the source repo still shows every
  task checkout regardless of path, so discoverability is kept.
- No provider-matrix expansion: agent kinds stay the existing taxonomy.
- Clarification scan: no `[NEEDS CLARIFICATION]` markers remain; scope
  decisions above (MCP/cost/kanban/inject deferrals) are made, not open.
