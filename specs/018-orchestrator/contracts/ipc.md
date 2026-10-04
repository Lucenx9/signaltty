# IPC + CLI contract: orchestrator

Follows existing rules (docs/08): typed `params.rs` decode (unknown fields
ignored, mistyped → `BAD_PARAMS`), codes from `signaltty-proto::code`, every
mutating result returns opaque ids, state changes emit journaled events.

## New error codes (added to `signaltty-proto::code`)

| Code | Meaning |
|---|---|
| `NO_SUCH_TASK` | Unknown task id (follows `NO_SUCH_*` pattern) |
| `AGENT_BUSY` | Submit refused: pane `working` or `blocked` (transient — wait + retry) |
| `AGENT_NOT_READY` | Submit refused: pane `unknown`/`failed`/never-started (needs inspection) |
| `MERGE_CONFLICT` | `task.finish --merge` aborted on conflict; target verified clean |
| (reused) `RATE_LIMITED` | `task.start` over the concurrency cap |
| (reused) `TIMEOUT` | Submit stall at the activity gate (`details.stage: "activity_gate"`) or `task.wait` expiry |
| (reused) `PANE_EXITED` | Submit to an exited pane |
| (reused) `PANES_ALIVE` | Finish/discard with foreign live panes inside the worktree |
| (reused) `BAD_PARAMS` | Empty objective/text, bad state for the operation, dirty-tree refusal, already finished, second report |

## Changed methods

### `pane.spawn` — optional lineage + label

New optional params: `parent_pane_id?`, `label?` (`≤ 128 chars`),
`relationship?` (`fork`|`subagent`, default `subagent` when a parent is set).
Unknown parent → `NO_SUCH_PANE`. Result gains nothing (lineage visible via
`pane.get`); child env gains `SIGNALTTY_PARENT_PANE` / `SIGNALTTY_TASK` when
set. `$SIGNALTTY_PANE` semantics unchanged.

### `pane.read` — rendered grid + incremental mode

`tail` keeps its `{text, truncated}` shape but is now cursor-correct: lines come
from the rendered grid + scrollback ring, not raw `\n` splitting. `screen`
unchanged.

New `mode: "rendered"`: `{pane_id, mode: "rendered", after_seq?, lines?}` →
`{text, seq, next_seq, dropped, truncated}`.

- `lines?` default 200, max 5000. `after_seq?` default 0 (from oldest retained).
- `seq` = oldest retained seq in this reply (or head when empty);
  `next_seq` = current head; pass it back as the next `after_seq`.
- `dropped: true` when `after_seq` < oldest retained (evicted, or any cursor
  from before a restart — cursors are runtime-only, like `output_offset`).
- `truncated: true` when the new range exceeds `lines` (returns the newest
  `lines`, still cursor-continuous).
- Unknown pane → `NO_SUCH_PANE`. Reads are passive: never refused on agent
  state, never move the viewport.

## New methods

### `pane.submit` — gated prompt delivery (herdr `agent.prompt`)

`{pane_id, text, submit_delay_ms?, stall_timeout_s?}` →
`{submitted: true, outcome, transition_seq, lifecycle, attention}`.

- `text`: 1 byte … 32 KiB, else `BAD_PARAMS`. `submit_delay_ms` default 300
  (herdr `AGENT_PROMPT_SUBMIT_DELAY`, `src/app/api/agents.rs`); `stall_timeout_s`
  default 5 (herdr `AGENT_PROMPT_EFFECT_TIMEOUT_MS`, same file).
- Gate (before any write): pane must be live and lifecycle `idle`|`done`
  (or `blocked` with attention `input_required` and no `pending_decision`).
  `working` → `AGENT_BUSY`; `blocked` with `permission_required` or a pending
  decision → `AGENT_BUSY`; `unknown`|`failed` → `AGENT_NOT_READY`;
  exited → `PANE_EXITED`. A worker pane whose task is still `pending`
  (background first-submit in flight) → `AGENT_NOT_READY` before any write.
  Answering a question/permission uses
  `decision.answer` / `pane.input`, never submit.
- Write: `ESC[200~ text ESC[201~`, wait the delay, then `\r` (research-codebase
  §2: `\n` does not submit in raw-mode TUIs; bracketed paste keeps multiline
  text out of premature execution). Text that already contains `ESC[200~` or
  `ESC[201~` is `BAD_PARAMS` and is not written. The markers are not stripped:
  dropping them would change the prompt the worker sees, and a split tail
  would still leave paste mode and run as keystrokes. The same check applies
  to the background first submit (`submit_refused` when that step hits it).
- Activity gate: a `working` or `blocked` transition newer than the pre-submit
  baseline within the stall budget (existing `WaitBaseline` machinery,
  research-codebase §3; fast completions before gate attach still match).
  Expiry → `TIMEOUT` with `details.stage: "activity_gate"`.
- Does not track turns beyond the gate (herdr CLI spec); use `wait` for the
  settled state. An accepted submit on a worker pane whose task is
  `input_required` moves the task back to `working` (follow-up path, FR-023).
  `pane.input` is unchanged as the raw-bytes escape hatch.

### `attention.pending` — ranked needs-user list

`{limit?}` (default 50, max 500) →
`{panes: [{pane_id, workspace_id, tab_id, label?, task_id?, lifecycle,
attention, last_message?, attention_since?}]}` ranked by docs/03 severity
(`error > permission_required > input_required > warning > unread`) then
recency — the same order `focus.next_unread` uses for its single winner
(agrees with herdr's blocked → unseen-done → working → seen-idle rank,
`src/app/api_helpers.rs`). Panes with `attention: none` are excluded.

### `task.start`

```json
{"contract": {"objective": "…", "constraints?": "…",
  "acceptance_criteria?": ["…"], "output_format?": "…"},
 "agent?": "claude|codex|…", "argv?": ["…"], "label?": "…",
 "parent_pane_id?": "pane_…", "context_id?": "tctx_…",
 "repo": "/abs/path", "base_ref?": "HEAD", "fetch_first?": false,
 "branch?": "…", "path?": "/abs/worktree/path",
 "ready_timeout_s?": 30, "stall_timeout_s?": 5}
```

→ `{task, pane}` with the task `pending`. Steps 1–4 run synchronously (fast,
bounded); step 5 runs server-side in the background (owned by the server —
survives the caller's disconnect; cancelled cleanly by `task.cancel`, discard,
or shutdown). Async start serialises nothing: N back-to-back starts each return
at `pending` (A2A: the task object is returned immediately, progress is observed
via state; VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/ — and
workmux's worktree-exists-before-agent so `wait` can start early,
research-external §workmux). Steps, in order:

1. Cap check: non-terminal task count < `max_parallel_tasks` (default 4; server
   `--max-tasks` / config), else synchronous `RATE_LIMITED` error — before
   creating anything (backpressure stays synchronous even though start is async).
2. Ref & branch validation: `base_ref` and `branch` (if provided) are validated
   against git reference rules via `git check-ref-format` (rejecting leading `-`,
   `--`, and illegal ref control characters; branch names containing `refs/` or
   `@{` are rejected, and a name whose `--branch` expansion differs from the
   input (e.g. `@{-1}`) is rejected; returns synchronous `BAD_PARAMS` before
   any git or disk operations).
   Resolve `base_ref` (default `HEAD`; `fetch_first` runs `git fetch origin`
   first, Conductor-style) → record `base_sha` AND `target_branch` = the source
   repo's currently checked-out branch (detached HEAD → `target_branch` unset).
   Empty repo / unresolvable ref → synchronous `BAD_PARAMS` (nothing on disk
   yet; pre-worktree validation creates no task). `contract.validate()` runs
   before the slot reservation: an empty or over-32 KiB objective is
   synchronous `BAD_PARAMS` and creates nothing. After the branch and base
   sha exist, and still before the worktree, the composed worker prompt must
   fit `32 KiB + preamble` for that task (the pane.submit cap stays 32 KiB;
   the extra bytes are only the fixed preamble, so a maximum objective with
   no constraints, criteria, or output format can be sent). A larger composed
   body is `BAD_PARAMS` and creates no worktree.
3. Under the per-path lock (vibe-kanban path mutex,
   `crates/worktree-manager/src/worktree_manager.rs`):
   `git worktree add -b <branch> <path> <base_sha>` (default branch
   `signaltty/<sanitized-label>-<shortid>`, default path
   `$XDG_DATA_HOME/signaltty/worktrees/<repo-dirname>-<short-hash-of-repo-path>/
   <sanitized-branch>` with slashes→dashes à la t3code
   `GitVcsDriverCore.createWorktree`; fallback `~/.local/share`; explicit `path`
   wins). Existing branch → adopt with
   `preexisting_branch: true` (claude-squad `isExistingBranch`).
4. `pane.spawn` in the worktree (`argv` or interactive default for `agent`,
   with existing ADR-0013 integration prep), carrying parent/label/task env.
   Steps 2–4 failures persist a `failed` task with `{stage:
   workspace|tab|parent|spawn, error, …}` evidence and return the error with
   `details: {task_id, stage}` (a created worktree is kept on disk, unlocked
   so `task.finish --discard` or plain `git worktree remove` deletes it).
5. Background: wait for `idle` within `ready_timeout_s`, then submit the
   composed worker prompt (preamble + objective + constraints + acceptance criteria + expected output format via `compose_worker_prompt`) via the `pane.submit` path (same gate + activity check).
   `submit_delay_ms` (default 300, the same default as `pane.submit`) is the
   pause between the bracketed paste and Enter.
   Success → task `working`; ready-timeout or submit failure → task `failed`
   with `{stage: ready_timeout|submit_refused|activity_gate, …}` evidence; pane
   kept. Observe via `task.wait --until working` or `task.*` events.

### `task.get` / `task.list`

`task.get {task_id}` → `{task}` (`NO_SUCH_TASK` when unknown).
`task.list {context_id?, state?, limit?}` (default limit 100, max 1000) →
`{tasks: […]}` ordered by creation.

### `task.wait`

Exactly one of `task_id` / `context_id`; `{…, until?, timeout_s?}` →
`{satisfied: true, tasks: […]}` or `TIMEOUT`.

- `until`: task state(s), or pseudo-states `terminal` (any of
  completed/failed/canceled/rejected) and `settled` (`terminal` OR
  `input_required` — the orchestrator default: a worker that ended its turn
  without reporting needs the requester to act, so a wait that slept through it
  would hang the loop). Default `settled`. Any other value (or a
  non-string) → `BAD_PARAMS`.
- `context_id` waits until ALL tasks in the context match (covers "wait on
  all"); empty context matches immediately. Unknown task → `NO_SUCH_TASK`.
- Connection-bound like `wait` (single-flight; EOF/shutdown cancels); does not
  survive restart — task state does, the waiter reconnects and re-reads.

### `task.report` — structured result write side

`{task_id?, pane_id?, status, summary, artifacts?, evidence?}` → `{task}`.

- Task resolution: explicit `task_id`, else the task owning `pane_id`
  (worker pane recorded at start; `$SIGNALTTY_TASK` / `$SIGNALTTY_PANE` feed
  this). Unknown → `NO_SUCH_TASK`.
- `status: completed|failed|rejected`; `summary` 1 byte … 8 KiB;
  `artifacts[]` ≤ 32 `{name, path, version?}`; `evidence {base_sha?, head_sha?,
  tests?, note?}`. Report on a terminal task → `BAD_PARAMS` (A2A immutable
  terminal).
- Transitions working/input_required/pending → the reported state, stores the
  result, emits `task.result` + `task.updated`.

### `task.diff` / `task.file_diff` — vs recorded base

Mirror the `workspace.diff` / `workspace.file_diff` pair, with the task's
`base_sha` as baseline (claude-squad diff-vs-`baseCommitSHA`, t3code checkpoint
0 — NOT worktree-vs-HEAD).

- `task.diff {task_id}` → `{task_id, base_sha, files[…], dirs[…], added,
  removed}` (tracked `git diff base_sha --numstat` + untracked via `ls-files`).
  Untracked line counts use a capped `O_NOFOLLOW` read (512 KiB, the same
  bound as `task.file_diff`) on a blocking thread. A symlink, fifo, directory,
  or file over that bound is `binary` with zero counts and its body is not
  loaded. Untracked text is shown as all-additions.
- `task.file_diff {task_id, path}` → same `content` shapes as
  `workspace.file_diff` (text hunks / binary / unchanged / unavailable).
- MUST NOT run `git add -N` or any index mutation (decision vs claude-squad
  `session/git/diff.go`). Missing base commit → `IO_ERROR` with cause.

### `task.finish` — explicit merge or discard (workmux rules)

`{task_id, mode: "merge"|"discard", target_ref?, delete_branch?, ignore_dirty?}`
→ `{task, merge?, cleanup_error?}`.

Merge (`src/workflow/merge.rs` rules, VERIFIED workmux):

- Requires task `completed`, else `BAD_PARAMS` with `details.state`.
- Target resolution: `target_ref` if given, else the recorded `target_branch`
  (absent after a detached-HEAD start → explicit `target_ref` required,
  `BAD_PARAMS` otherwise). The resolved target MUST be the branch currently
  checked out in the source repo; otherwise refuse with `BAD_PARAMS` +
  `details: {expected, actual}` — never merge into whatever branch the user
  switched to since start.
- Source checks: any porcelain output (staged/unstaged/untracked) → refuse
  unless `ignore_dirty: true` ("those changes would be lost"). A `git status`
  that itself fails (non-zero exit, including dubious ownership or a missing
  `.git` pointer) is `IO_ERROR`, never "clean": finish merges or deletes
  nothing on an unverified tree. Staged work is never auto-committed
  (decision vs workmux's editor commit: non-interactive server, no invented
  authorship — commit explicitly first).
- Target checks (on the resolved target above): tracked dirt → refuse. Untracked on target allowed (git fails later
  on collision, as workmux).
- `git merge --no-ff --no-edit -- <branch>` in the target checkout (`--`
  guards branch names that parse as revisions; `branch -D` takes the same
  guard). Conflict →
  `git merge --abort`, verify target clean via porcelain, return
  `MERGE_CONFLICT` with `details.conflicted[]` (`git diff --diff-filter=U`,
  vibe-kanban `crates/git`) — target stays clean, resolution stays in the
  source worktree. If `merge --abort` fails, or the post-abort porcelain
  check errors or still shows tracked dirt, return `IO_ERROR` with
  `details.target_dirty` and `details.abort_ok` and do not claim the target
  is clean.
- After a successful merge, the new `HEAD` must descend from the task branch
  tip (`merge-base --is-ancestor`), else finish returns `IO_ERROR` and records
  no disposition — a merge that resolves to the already-checked-out target
  reports success without landing task commits.
- Success: `merge: {target, sha}`; cleanup removes the task worktree
  (guarded to the recorded path + herdr leftover guard: delete a leftover dir
  only after git says not-a-worktree and the checkout still matches,
  `src/worktree.rs`); branch deleted only when `delete_branch: true` AND
  `!preexisting_branch` (t3code keeps branches for resume; claude-squad never
  deletes pre-existing). Default keeps the branch (decision vs workmux
  `keep_branch: false`: durable task records favor post-merge inspection).
- Cleanup failure → success result with `cleanup_error` set (workmux `Ok` +
  `cleanup_error`); the merge is not rolled back in the report. t3code skips
  also apply: refuse cleanup while a foreign live pane's canonical cwd is
  the worktree or a directory inside it (`PANES_ALIVE`; whole path components
  after canonicalize, so `…/proj-other` does not match and a symlink does) or
  porcelain shows branch mismatch / moved HEAD vs the merge
  result.
- Records `disposition: merged` only through the store transition
  (`task_finish_record`), which re-checks `outcome == none` under the
  per-path lock and emits `task.updated` with a top-level `task_id`.
  Cleanup failure and conflict files are stored on `finish_error` (cleanup
  still returns success with `cleanup_error`; conflicts stay `MERGE_CONFLICT`
  and do not set a disposition). Second finish → `BAD_PARAMS` once the
  recorded worktree path is gone. If that path is still present, the second
  finish retries cleanup only and does not merge again.

Discard:

- Allowed from any state without a disposition (working tasks: worker pane
  closed first with SIGTERM). Refuses foreign live panes whose canonical cwd
  is inside the worktree (`PANES_ALIVE`, same component boundary as merge),
  refuses non-recorded paths. Force-removes ONLY the recorded
  worktree path (discard is explicit destruction; vibe-kanban-grade force is
  scoped to the task path, never a default elsewhere).
- Branch kept unless `delete_branch: true` (still never when pre-existing).
- Records `disposition: discarded` (+ `canceled` transition when from
  non-terminal) through the same store transitions, with `task_id` on the
  event. A failed removal is `finish_error.cleanup_error` and can be
  retried by a later finish while the path remains.

### `task.cancel`

`{task_id}` → `{task}`. Non-terminal → `canceled`, worker pane closed,
checkout + branch preserved for inspection (the "stop, keep everything" path).
Terminal → `BAD_PARAMS`. (Discard = cancel + remove worktree; separate verbs,
separate intents.)

### Hook receiver and adapter integration

- **Harness mismatch drop**: When a pane has been identified with a specific agent
  harness (e.g. `codex`), hook events from an incompatible harness (e.g. `claude`)
  are rejected without modifying pane state or agent kind:
  `{"accepted": false, "dropped": true, "reason": "harness_mismatch"}`.
- **Unparseable / unrecognized hook drop**: Malformed or unhandled hook payloads that
  do not convey lifecycle, attention, session identity, notification, or decision
  data are dropped without mutating state:
  `{"accepted": false, "dropped": true, "reason": "unrecognized_hook"}`.
- **`last_assistant_message` capture**: `Stop` and `agent-turn-complete` hook payloads
  containing `last_assistant_message` record this text into pane `last_message` and
  task evidence `last_message` when a turn ends without a structured report.
- **Permission & Notification text flow**: `PermissionRequest` and `Notification` prompt
  or message text flows into pane `last_message` and appears immediately in
  `attention.pending` and UI listings to indicate what action is blocked.

### Silent-worker watchdog

- A task in `working` state with no hook events and no PTY output for
  `worker_silent_timeout_s` (config `worker_silent_timeout_s`, CLI `--worker-silent-timeout`,
  environment `SIGNALTTY_WORKER_SILENT_TIMEOUT_S`, default 600s / 10 minutes; set to `0`
  to disable) automatically transitions to `input_required` with
  `status_reason: {"reason": "worker_silent", "timeout_s": N}` so the orchestrator
  wait loop is notified rather than hanging indefinitely.

## Events

All carry `seq`, are journaled state events (016 replay rules apply), and
include `task_id` + `context_id`:

```text
task.created  {task}
task.updated  {task_id, context_id, task, prev_state?}
task.result   {task_id, context_id, result}
```

`subscribe` gains the `task.*` glob and an optional `task_ids[]` server-side
filter composing with `from_seq` replay (task-scoped subscribe/replay).

## CLI surface (thin wrappers, `--json` result passthrough)

| CLI | Method |
|---|---|
| `signaltty task start --repo … --objective/--objective-file … [--agent\|--argv …] [--label …] [--branch …] [--path …] [--base-ref …] [--fetch-first] [--context …] [--json]` (returns `{task, pane}` at `pending`; background ready + submit observed via `wait`/`get`) | `task.start` |
| `signaltty task get <id> [--json]` | `task.get` |
| `signaltty task list [--context …] [--state …] [--json]` | `task.list` |
| `signaltty task wait <id\|--context …> [--until settled] [--timeout …] [--json]` (default `settled` = terminal or `input_required`) | `task.wait` |
| `signaltty task diff <id> [--json]` / `signaltty task file-diff <id> <path> [--json]` | `task.diff` / `task.file_diff` |
| `signaltty task finish <id> --merge\|--discard [--target …] [--delete-branch] [--ignore-dirty] [--json]` | `task.finish` |
| `signaltty task cancel <id> [--json]` | `task.cancel` |
| `signaltty report --status … --summary … [--artifacts …] [--evidence …] [--task …] [--json]` (task resolved via `--task`, `$SIGNALTTY_TASK`, or `$SIGNALTTY_PANE`) | `task.report` |
| `signaltty pane submit <id> --text/--stdin … [--submit-delay-ms …] [--stall-timeout …] [--json]` | `pane.submit` |
| `signaltty pane read <id> --mode rendered [--after-seq N] [--lines N] [--json]` | `pane.read` |
| `signaltty pane spawn … [--parent-pane …] [--label …] [--relationship …]` | `pane.spawn` |
| `signaltty attention [--limit N] [--json]` | `attention.pending` |

## MCP follow-up mapping (deferred server, 1:1 contract)

When the MCP server lands, each tool maps to exactly one IPC method with the
same params/result JSON: `task_start→task.start`, `task_get→task.get`,
`task_list→task.list`, `task_wait→task.wait`, `task_report→task.report`,
`task_diff→task.diff`, `task_file_diff→task.file_diff`,
`task_finish→task.finish`, `task_cancel→task.cancel`,
`pane_submit→pane.submit`, `pane_read→pane.read`,
`attention_pending→attention.pending`. No reshaping allowed — the IPC contract
above is already the tool schema.
