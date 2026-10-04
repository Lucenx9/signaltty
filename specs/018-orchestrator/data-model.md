# Data model: orchestrator tasks

Domain additions for feature 018. Existing `Pane`/`Workspace`/`Tab` shape is
unchanged apart from the serde-defaulted fields below (snapshot invariant from
research-codebase §7: new fields MUST `#[serde(default)]` or old snapshots
fail to load).

## Task

Top-level entity in `signaltty-core/src/model.rs`, stored in
`Store.tasks: HashMap<String, Task>`, persisted as
`#[serde(default)] tasks: Vec<Task>` on `Snapshot`.

| Field | Type | Meaning |
|---|---|---|
| `id` | `task_…` opaque | UUIDv4 with prefix, as other ids |
| `context_id` | `tctx_…` opaque | A2A collaboration id; follow-ups are new tasks, same context (VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/) |
| `parent_task_id` | optional | Refinement link (`referenceTaskIds` analogue) |
| `pane_id` | optional | Worker pane; absent before spawn / after pane removal |
| `parent_pane_id`, `root_pane_id` | optional | Lineage written at spawn (t3code `OrchestrationV2ThreadShell.lineage`, `packages/contracts/src/orchestrationV2.ts`) |
| `relationship` | `fork`\|`subagent` | Default `subagent`; provider-native children never move the parent pane lifecycle (cmux `isSubagent` ignored for badge, `AgentLifecycleReducer.swift`) |
| `label` | string | Human label, set at create (workmux handle, `src/state/types.rs`) |
| `contract` | Contract | Objective + constraints + acceptance + output format |
| `agent` | optional kind | Requested agent taxonomy kind |
| `source_repo` | path | Repo the worktree was cut from; merge target lives here |
| `target_branch` | optional string | Source repo's checked-out branch recorded at start (detached HEAD → absent; then finish requires explicit `target_ref`) |
| `worktree_path` | path | Task-owned checkout |
| `branch` | string | Task branch |
| `preexisting_branch` | bool | True when the branch already existed (claude-squad `isExistingBranch`, `session/git/worktree.go`); never auto-deleted |
| `base_ref`, `base_sha` | strings | Requested ref + resolved full SHA at create (claude-squad `BaseCommitSHA`; t3code checkpoint ordinal 0, `CheckpointDiffQuery.ts`) |
| `state` | TaskState | A2A-aligned lifecycle below |
| `result` | optional Result | Structured handoff, set once by report |
| `disposition` | Disposition | Review outcome metadata (NOT lifecycle — terminal states stay immutable) |
| `status_reason` | optional | A2A-style failure / interrupt evidence ({stage\|reason,...}, input_required evidence) |
| `finish_error` | optional | Failed-finish evidence. Conflict files (`conflicted[]`, plus `target_dirty` / `abort_ok` when abort does not leave the target clean) are stored without a disposition so finish can be retried. A cleanup failure is `{cleanup_error}` on a recorded disposition; the next finish retries removal while `worktree_path` still exists and clears this field when removal succeeds. |
| `worker_pid`, `worker_cmd` | optional | Crash-vs-recycle evidence (workmux `AgentState.pane_pid/command/boot_id`, `src/state/types.rs`) |
| `client_request_id` | optional | Caller idempotency key from `task.start`; the same key returns the existing task (persisted, survives restart) |
| `created_at`, `updated_at` | timestamps | |

### TaskState

`pending → working ⇄ input_required → completed | failed | canceled | rejected`.
A2A-aligned (VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/):
working, interrupted (`input_required`), terminal
(`completed`/`canceled`/`rejected`/`failed`), terminal immutable, refinements
are new tasks in the same context.

| Transition | Producer |
|---|---|
| `pending → working` | background ready-wait + submit accepted (server-owned; survives client disconnect; cancelled by cancel/discard/shutdown) |
| `pending → failed` | spawn/submit/ready failure, with evidence (`{stage, …}`; restart recovery uses `stage: "restart"`) |
| `pending/working/input_required → canceled` | `task.cancel`, or `finish --discard` from non-terminal |
| `working ⇄ input_required` | worker blocked hook ⇄ answer/activity; ALSO `working → input_required` on worker lifecycle `done` with no report (`{reason: "turn_ended_without_report", last_message?}`), on silent-worker watchdog expiry (`{reason: "worker_silent", timeout_s}`), and `input_required → working` on an accepted follow-up submit. Only `done` ends a turn (`idle`, e.g. a late `SessionStart`, does not). A `Stop` that lands while the task is still `pending` (first submit in flight) or `input_required` (follow-up in flight) is applied when the submit commits: entering `working` with a `done` pane and no report parks the task in `input_required` with turn-end evidence in the same write. |
| `working/input_required → completed/failed/rejected` | `task.report` status |
| `working/input_required/pending → failed` | worker pane death, restart recovery (evidence, once) |

`rejected` is worker-side: "finished but does not meet acceptance"
(A2A: the agent rejects the task). User-side refusal of finished work is
`disposition: discarded` on a `completed` task — no transition out of terminal.

### Contract

| Field | Rule |
|---|---|
| `objective` | Required, 1 byte … 32 KiB (body cap after cmux `AgentMessage` 32 KiB, `AgentMessage.swift`) |
| `constraints` | Optional free text |
| `acceptance_criteria` | Optional list (Anthropic: vague briefs → duplication/gaps; VERIFIED https://www.anthropic.com/engineering/multi-agent-research-system/) |
| `output_format` | Optional free text (the worker's result shape contract) |

No tool/file budgets in v1 (deferred with cost budgets — no enforcement point).

### Worker prompt composition

The submitted worker prompt is composed via `signaltty_core::model::compose_worker_prompt` (or `Task::compose_worker_prompt`):
- Preamble: `"You are a worker for task <id> in an isolated worktree on branch <branch> from base <sha>; stay inside this worktree; commit your work; when done or blocked run signaltty report --status completed|failed|rejected --summary … with evidence — it reads $SIGNALTTY_TASK.\n\n"`
- Objective: `"## Objective\n<contract.objective>\n"`
- Constraints (if present): `"\n## Constraints\n<contract.constraints>\n"`
- Acceptance Criteria (if present): `"\n## Acceptance Criteria\n- <item>\n"`
- Expected Output Format (if present): `"\n## Expected Output Format\n<contract.output_format>\n"`

`task.start` calls `Contract::validate` before reserving a slot or creating a
worktree. `pane.submit` still rejects text over 32 KiB. The background first
submit may send `32 KiB + preamble`, where the preamble is the fixed wrapper
for that task id, branch, and base sha. A maximum objective with no extra
sections fits. Extra sections that push the composed body over that limit are
`BAD_PARAMS` and create no worktree.

### Result (structured handoff)

Set once by `task.report`; second report on a terminal task is refused
(A2A immutable terminal).

| Field | Rule |
|---|---|
| `status` | `completed`\|`failed`\|`rejected` |
| `summary` | Required, ≤ 8 KiB |
| `artifacts` | ≤ 32 entries of `{name, path, version?}` (A2A named artifacts; client tracks versions) |
| `evidence` | `{base_sha, head_sha?, tests?, note?}` free-ish shape |
| `reported_at` | timestamp |

This payload is forwards-compatible with a future cmux-style parent inbox
(delivery states would wrap it, not reshape it).

### Disposition

`{outcome: none | merged | discarded, target_ref?, merged_sha?, branch_deleted?, at?}`.
Recorded once by `task.finish`; a second finish is refused. Merge details
(target, SHA) support resume-as-new-task and audit.

## Pane additions (all serde-defaulted)

`parent_pane_id?`, `root_pane_id?` (server-derived: parent's root else parent
else self), `label?`, `relationship?`, `task_id?`. Env at spawn:
`SIGNALTTY_PANE` unchanged (own id), plus `SIGNALTTY_PARENT_PANE` and
`SIGNALTTY_TASK` when set (extends `pty.rs` injection; `SIGNALTTY_*` prefix is
already client-allowlisted).

## Read cursor (runtime only, like `output_offset`)

Per-pane monotonic `content_seq` over rendered lines (starts at 1, +1 per
rendered line appended to scrollback/screen ring). Not persisted; after restart
any old cursor reads as dropped. Ring capacity 5000 rendered lines per pane.

## Invariants

- Terminal task states never transition; disposition/finish metadata is not a
  transition.
- One writer per state directory (existing rule) extends to the per-path git
  lock: worktree create/remove/finish for the same path serialize.
- `task.start` failures after worktree creation leave the checkout on disk and
  the task failed with evidence (t3code `onWorktreeClaimed`: registered path is
  safe to keep; `GitVcsDriverCore.ts`).
- Pane exit for a worker pane of a non-terminal task fails the task exactly
  once (transition method owns emit + persistence marking).
- All task transitions (including background `pending → working/failed`,
  turn-end `working → input_required`, and restart-recovery fails) MUST go
  through `Store` transition methods that pair mutation + emit (AGENTS.md rule)
  — no direct state writes from the background step, hook handlers, or recovery.
- Default worktree path: `$XDG_DATA_HOME/signaltty/worktrees/<repo-dirname>-
  <short-hash-of-repo-path>/<sanitized-branch>` (slashes→dashes, à la t3code
  `GitVcsDriverCore.createWorktree` default `worktreesDir/repoBasename/
  sanitizedBranch; research-external §t3code). `XDG_DATA_HOME` fallback
  `~/.local/share`. New `signaltty-core::paths::data_dir()` follows the existing
  `state_dir`/`config_dir` pattern. Explicit `path` still wins; cleanup stays
  guarded to the recorded path, so the new default changes nothing about
  removal safety.
- Over-cap `task.start` creates nothing (cap checked before base resolution).
