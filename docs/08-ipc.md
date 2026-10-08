# 08 — IPC Schema (`signaltty/1`)

Transport: Unix domain socket at
`$XDG_RUNTIME_DIR/signaltty/signaltty.sock` (mode 0700 dir, 0600
socket, owned by user). Framing: newline-delimited JSON (JSONL), one
request per line, one response per line on the same connection;
server→client events are JSONL messages with `type: "event"`.
Binary PTY data inside events is base64. Max line 16 MiB (PTY bursts
are chunked).

## Envelope

Request:

```json
{"protocol":"signaltty/1","id":"c1","method":"pane.spawn","params":{…}}
```

Response (ok):

```json
{"protocol":"signaltty/1","id":"c1","ok":true,"result":{…}}
```

Response (error):

```json
{"protocol":"signaltty/1","id":"c1","ok":false,"error":{"code":"NO_SUCH_PANE","message":"…","details":{}}}
```

Event:

```json
{"protocol":"signaltty/1","type":"event","event":"agent.done","seq":412,"payload":{"pane_id":"pane_…"}}
```

Rules: `protocol` mismatch → `BAD_PROTOCOL` error, connection stays
open. Unknown method → `UNKNOWN_METHOD`. Unknown fields ignored
(forward compat); present-but-mistyped fields → `BAD_PARAMS` (missing
and explicit null still mean "absent" for optionals). Every mutating
result returns opaque ids. `seq` is a monotonic server counter;
clients can `subscribe {from_seq}` to replay.

## Methods

| Method | Params | Result |
|---|---|---|
| `server.status` | — | `{version, protocol, uptime_s, workspaces, tabs, panes, live_panes}` |
| `server.shutdown` | `{force?}` | `{stopped}` (refuses with `PANES_ALIVE` unless force) |
| `server.schema` | — | `{protocol, version, methods[], events[], codes[]}` (self-printing contract, same constants the router matches on) |
| `workspace.create` | `{name?, cwd?}` | `{workspace}` |
| `workspace.list` | — | `{workspaces:[…]}` |
| `workspace.get` | `{workspace_id}` | `{workspace, tabs:[…]}` |
| `workspace.rename` | `{workspace_id, name}` | `{workspace}` |
| `workspace.close` | `{workspace_id, signal?}` | `{closed}` |
| `workspace.refresh_git` | `{workspace_id}` | `{workspace}` |
| `workspace.diff` | `{workspace_id}` | `{workspace_id, branch?, files[{path, added, removed, untracked, binary}], dirs[{dir, added, removed}], added, removed}` (worktree-vs-HEAD `git diff --numstat` as data; non-repo → `BAD_PARAMS`) |
| `workspace.file_diff` | `{workspace_id, path}` | `{workspace_id, path, untracked, content}` (one literal root-relative filename; typed text hunks or binary/unchanged/unavailable state; current worktree against HEAD) |
| `worktree.list` | `{workspace_id}` | `{worktrees:[{path, branch?, head?, main, bare, locked, prunable, workspace_id?}]}` (actual Git registrations and open workspace association) |
| `worktree.create` | `{workspace_id, path, branch, name?}` | `{workspace, path, reused:false}` (absolute new checkout path, new branch at source HEAD; checkout retained if subsequent workspace binding fails) |
| `worktree.open` | `{workspace_id, path, name?}` | `{workspace, path, reused}` (absolute registered checkout path, canonical-cwd workspace reuse) |
| `worktree.remove` | `{workspace_id, path}` | `{removed:true, path}` (explicit non-force removal; main, dirty, locked and open-referenced checkouts refused; branch retained) |
| `tab.create` | `{workspace_id, title?}` | `{tab}` |
| `tab.close` | `{tab_id, signal?}` | `{closed}` |
| `tab.set_layout` | `{tab_id, layout}` | `{tab}` |
| `tab.set_ratio` | `{tab_id, path, ratio}` | `{tab}` |
| `pane.spawn` | `{workspace_id, tab_id?, cwd?, argv, env?, cols?, rows?, agent_hint?, parent_pane_id?, label?, relationship?, task_id?}` | `{pane, integration?}` |
| `pane.split` | `{pane_id, direction: "right"\|"down", argv?, cwd?}` | `{pane, integration?}` (new sibling) |
| `pane.get` | `{pane_id}` | `{pane, wait_baseline}` |
| `pane.input` | `{pane_id, data_b64}` | `{written}`; five-second input budget, stalled input → `TIMEOUT`, closed bound PTY → `PANE_EXITED`; write failures carry `details.written_bytes` (kernel-accepted prefix, `null` if a worker failure makes it unknown); no automatic retry |
| `pane.resize` | `{pane_id, cols, rows}` | `{pane}` |
| `pane.signal` | `{pane_id, signal, group?}` | `{sent}` (`INT TERM KILL HUP QUIT WINCH USR1 USR2`, case-insensitive, optional `SIG`; any other name, here or in a `*.close` `signal`, → `BAD_PARAMS`) |
| `pane.read` | `{pane_id, mode: "screen"\|"tail"\|"rendered", lines?, strip_ansi?, after_seq?}` | `screen`/`tail` → `{text, truncated}`; `rendered` → `{text, seq, next_seq, dropped, truncated}` |
| `pane.attach` | `{pane_id, cols?, rows?, mark_seen?}` | `{snapshot_b64, output_offset, size, live, ...}` then `pty.data` stream; the snapshot is replayable VT state (contents, colours, cursor, modes) (`mark_seen` default true; GUIs pass false and acknowledge on focus) |
| `pane.detach` | `{pane_id}` | `{detached}` (also implicit on disconnect) |
| `pane.close` | `{pane_id, signal?}` | `{closed}` |
| `pane.resume` | `{pane_id}` | `{pane, integration?}` (spawns adapter resume argv; errors unless restored+resumable) |
| `pane.mark_seen` | `{pane_id}` | `{pane}` (records reading; clears ordinary attention while preserving unanswered decisions and their required attention) |
| `pane.submit` | `{pane_id, text, submit_delay_ms?, stall_timeout_s?}` | `{submitted, outcome, transition_seq, lifecycle, attention}` (bracketed paste + delayed Enter; embedded `ESC[200~` / `ESC[201~` is `BAD_PARAMS` before any write, not stripped; gate checks lifecycle, resumes `input_required` to `working`) |
| `decision.answer` | `{pane_id, decision_id, option_id}` | `{answered, lifecycle?, attention?}` (delivers through a live native permission waiter or the pane adapter's channel and consumes the id; stale/consumed ids → `NO_SUCH_DECISION`, unknown option or channelless adapter → `BAD_PARAMS`) |
| `notify` | `{pane_id?, title, body?, severity?}` | `{notification}` |
| `hook-event` | `{agent, event, pane_id?, client_pid?, payload?, message?, title?, severity?, decision?}` | `{accepted, agent, event, pane_id, lifecycle?, attention?}` (adapter classification; pane by explicit id or `client_pid` ancestry; `decision: {id, prompt, options[{id, label}]}` sets/supersedes the pane's pending decision, captured `answerable` iff the adapter has a channel) |
| `report-session` | `{pane_id, agent_session_id, agent?}` | `{pane}` |
| `subscribe` | `{events?: ["agent.*","attention.*",…], from_seq?}` | `{subscribed, seq, replay?}` then complete replay/live stream |
| `wait` | `{pane_id, until: string\|string[], after?: wait_baseline, timeout_s?}` | `{satisfied, outcome, transition_seq, lifecycle, attention}` or `TIMEOUT` / `IDENTITY_CHANGED` |
| `focus.next_unread` | — | `{pane_id?}` (severity→recency order) |
| `task.start` | `{repo, contract: {objective, constraints?, output_format?, acceptance_criteria?}, agent?, label?, parent_pane_id?, context_id?, client_request_id?, base_ref?, fetch_first?, branch?, path?, ready_timeout_s?, stall_timeout_s?, submit_delay_ms?, argv?}` | `{task, pane}` (starts worker at `pending`; background ready-wait and prompt submit transitions to `working`; objective over 32 KiB, or a composed prompt over 32 KiB plus the fixed preamble, is `BAD_PARAMS` and creates no worktree; the same `client_request_id` returns the existing task and pane instead of creating another; `repo` and `path` must be absolute, and the task exists before its worker starts, so an immediate report finds it) |
| `task.get` | `{task_id}` | `{task}` |
| `task.list` | `{context_id?, state?, limit?}` | `{tasks: [...]}` |
| `task.wait` | `{task_id?, context_id?, until?: string\|string[], timeout_s?}` | `{satisfied, tasks: [...]}` (default `until: settled`) |
| `task.cancel` | `{task_id}` | `{task}` |
| `task.report` | `{task_id?, pane_id?, status: completed\|failed\|rejected, summary, artifacts?, evidence?}` (task resolved via explicit `task_id`, else the task owning `pane_id`; one of the two is required) | `{task}` (stores the result, emits `task.result` + `task.updated`; report on a terminal task → `BAD_PARAMS`) |
| `task.diff` | `{task_id}` | `{task_id, base_sha, branch, files[...], dirs[...], added, removed, merge_preview}` (`merge_preview: {target, clean, conflicted[]}` previews merging the committed branch tip into the recorded target, `null` when git cannot tell; worktree-vs-recorded-base, tracked + untracked; no index mutation; untracked reads are capped, `O_NOFOLLOW`, and off the runtime thread — symlinks, fifos, and files over 512 KiB are binary with zero counts) |
| `task.file_diff` | `{task_id, path}` | `{task_id, path, untracked, content}` (same `content` shapes as `workspace.file_diff`) |
| `task.finish` | `{task_id, mode: merge\|discard, target_ref?, delete_branch?, ignore_dirty?}` | `{task, merge?: {target, sha}, cleanup_error?}` (the outcome is `task.disposition.outcome` (`merged`/`discarded`), while `task.state` keeps its A2A value; a merge deletes the task branch unless `delete_branch: false`, discard keeps it unless `delete_branch: true`, and a pre-existing branch is never deleted; merge needs a `completed` task and a clean target; a failed `git status` is `IO_ERROR`, never "clean"; conflicts abort leaving the target clean → `MERGE_CONFLICT`, or `IO_ERROR` with `details.target_dirty` when abort fails; the merge runs in its own process group under a deadline (default 120 s, `SIGNALTTY_MERGE_TIMEOUT_MS`) and expiry kills it, restores the target and returns `TIMEOUT`, or `IO_ERROR` when it cannot; a merge git refuses without conflicts (hook, unrelated histories) is `IO_ERROR` carrying git's message; both modes kill and reap the worker's process group first, and once the worktree is gone the workspaces rooted at it close; an explicit `path` on `task.start` must be absolute) |
| `task.pr_open` | `{task_id, title?, body?, draft?}` | `{task}` (pushes task branch to origin, opens GitHub PR via `gh pr create`, records `task.pr: {number, url, state: open, checks: none, review: none}`; requires completed task with no disposition or PR; each `git`/`gh` call runs in its own process group with prompts disabled under a 60 s deadline → `TIMEOUT`, missing `gh` → `SPAWN_FAILED`, non-zero exit → `IO_ERROR` with `details.stderr`; nothing is recorded on failure) |
| `task.pr_refresh` | `{task_id?}` | `{tasks: [...]}` (refreshes PR state, reviewDecision, and CI statusCheckRollup via `gh pr view` for the given task (errors as `task.pr_open`) or all open PRs, where a failing PR is skipped and keeps its stored state; returns refreshed tasks) |
| `attention.pending` | `{limit?}` (default 50, max 500) | `{panes: [{pane_id, workspace_id, tab_id, label?, task_id?, lifecycle, attention, last_message?, attention_since?}]}` ranked by severity then recency; panes with `attention: none` excluded |
| `plugin.list` | — | `{dir, plugins[], failures[]}` (hooks with runs/errors/last_error; see 13) |
| `plugin.reload` | — | same as `plugin.list` after re-scan (stats reset) |

`until` accepts any lifecycle or attention state, plus `seen` and
`attention_cleared`; a nonempty array matches any alternative in caller order.
Without `after`, an already matching current state succeeds immediately.

`task.start` requires `agent` or `argv` (else `BAD_PARAMS`) and runs cap →
base → worktree → spawn synchronously: over-cap starts are refused with
`RATE_LIMITED` before creating anything. Validation and base/worktree failures
before the checkout exists return synchronous errors and create no task.
Once the checkout exists, workspace/tab/parent/spawn failures persist a
`failed` task and leave its worktree available for inspection or discard.
The error includes `details: {task_id, stage}`. Background ready-wait and
prompt-write failures also persist `failed` tasks with
`{stage: ready_timeout|submit_refused, …}` evidence (`ready_timeout` adds
`screen_tail`, the pane's last screen lines). The background step
waits for the worker pane to reach `idle`/`done` (default
`ready_timeout_s: 30`), writes the composed prompt with `submit_delay_ms`
(default 300, the same paste/Enter delay as `pane.submit`), and runs the
same activity gate: a newer `working` or `blocked` transition within
`stall_timeout_s` (default 5) moves the task to `working`; a stall (after one Enter retry halfway through the gate) parks it
at `input_required` with `{reason: submit_unconfirmed, stage: activity_gate}`,
and the worker turning `working` later resumes it. Closing a worker from
`task.finish`/`task.cancel` clears its attention; any pane exit drops
`input_required`/`permission_required` to `unread`. A repeated `client_request_id` returns the
existing task and pane instead of creating another; without it retries
create new tasks. The merge target defaults to the recorded
`target_branch`; when the start ran on a detached HEAD an explicit
`target_ref` is required at finish, and the resolved target must be the
branch currently checked out in the source repo (`BAD_PARAMS` with
`details: {expected, actual}` otherwise).

`pane.submit` rechecks decisions immediately before its delayed Enter. If a
permission request or pending decision arrived after paste, it returns
`AGENT_BUSY` with `details: {stage: "delayed_enter", paste_delivered: true}`
without writing Enter. The pasted text is already in the worker input box;
inspect and resolve the decision before sending more input. Automatic Enter
retries hold the same store read guard for the decision check and Enter write.
For a task's background first submit, a refusal after paste uses recoverable
`input_required` rather than failing the task. A current decision is recorded
as `decision_required`; when no decision remains, existing submit-unconfirmed
recovery applies. The pane's decision remains unanswered. If a decision arrives
after Enter but before submit confirmation,
the task commit preserves `input_required` and its `decision_required` evidence,
for both the background first prompt and a follow-up.

`task.wait` takes exactly one of `task_id` / `context_id` (both or neither
→ `BAD_PARAMS`); `until` accepts task states plus the pseudo-states
`terminal` (any of completed/failed/canceled/rejected) and `settled`
(terminal OR `input_required`, the default). A context wait ends when every
task in it matches (an empty context matches immediately); unknown tasks →
`NO_SUCH_TASK`, expiry → `TIMEOUT` (default timeout 3600 s). Unknown `until`
strings and an empty `until` array return `BAD_PARAMS`. Unlike pane `wait`, the
task waiter is a polling loop, not a single-flight connection: it does not
cancel on client disconnect.

`task.finish` records disposition through a store transition that emits
`task.updated` with a top-level `task_id`. Conflict file names and a cleanup
failure are stored on `task.finish_error`. A second finish is `BAD_PARAMS`
once the recorded worktree is gone; while that path still exists the second
finish retries removal and does not merge again. A foreign live pane blocks
finish only when its canonical cwd is the worktree or a directory inside it
(`PANES_ALIVE`); comparison is on whole path components, so a sibling such as
`…/proj-other` does not match and a symlink to the worktree does.

`subscribe` accepts an optional `task_ids[]` server-side filter alongside
`events`/`from_seq`: only events whose payload carries a matching `task_id`
are delivered (other events still pass the glob match).

`pane.read` modes: `screen` returns the current grid as plain text (no
scrollback). `tail` (default, `lines` default 200, max 5000) returns the
last content lines from the rendered ring — cursor-correct (CR overwrites,
repaints and alternate-screen grids resolved), same `{text, truncated}`
shape as before. `rendered` adds the incremental cursor: pass `after_seq`
(default 0 = oldest retained) and `lines` (default 200, max 5000); the
reply's `seq` is the oldest retained line in the reply (or the head when
empty), `next_seq` is the current head (pass back as the next `after_seq`),
`dropped` means `after_seq` is older than retained scrollback, ahead of the
head, or from before a restart (cursors are runtime-only, like
`output_offset` — re-read from scratch), and `truncated` means the new
range exceeded `lines` (the newest `lines` are returned, still
cursor-continuous). Unknown pane → `NO_SUCH_PANE`. Reads are passive:
never refused on agent state, never move the viewport. While a full-screen
TUI owns the alternate grid, reads return the current alt grid; repaints
bump the touched rows' sequences (poll again with `next_seq` for deltas)
and nothing is appended to history. See docs/05 (rendered ring).

`pane.get` adds `wait_baseline {pane_id, process_instance, agent_session_id,
session_generation, lifecycle_seq, attention_seq}`. Capture it **before** submitting
new work and pass the whole object as `wait.after`. A matching outcome requires a
newer transition on its own axis; attention changes cannot validate an old Done.
Fast work completed before wait arrival still matches, including brief outcomes
that have already moved on. `outcome`/`transition_seq` identify the matched transition;
`lifecycle`/`attention` report the current pane state. Repeated same-state hooks
do not create a new transition. A baseline does not identify individual concurrent
input writers; serialize submissions when turn attribution matters.

Process replacement (resume or server restart) and known session replacement return
`IDENTITY_CHANGED` before considering state. Exit retains process identity so
`until:exited` can succeed. First session discovery is allowed; subsequent replacement,
even replacement back to the original value, invalidates the baseline. Missing panes
remain `NO_SUCH_PANE`; malformed/ahead baselines and empty outcomes are `BAD_PARAMS`.
Wait uses a dedicated single-flight connection: EOF, another request on that connection
or shutdown cancels it immediately. Runtime schema advertises these capabilities.
See [ADR-0018](adr/0018-event-recovery-and-work-baselines.md).
Glob subscriptions: `*`, `agent.*`, `pane.*`, `workspace.*`, `tab.*`,
`attention.*`, `notification.*`, `decision.*`, `worktree.*`, `git.*`, `pty.*`,
`task.*`.

Worktree methods accept a source workspace ID or handle. Git owns registrations;
these methods do not fetch a remote or delete a branch. Close the worktree
workspace before removal. Live pane references return `PANES_ALIVE`; invalid,
dirty, main, locked, or otherwise open targets return `BAD_PARAMS`. A path
reservation prevents concurrent workspace creation or pane launch into a
checkout being removed. Git operations have a bounded execution deadline.

`hook-event` also accepts `wait_for_answer?:bool` and `wait_timeout_s?:1..120`.
The waiting mode requires a supported native `PermissionRequest` payload with
`session_id`, `tool_name` and object `tool_input`, and no explicit `decision`.
It registers a live answer route, publishes generated Allow once/Deny choices
and returns `{accepted:true, native_verdict}` when answered. Cancellation returns
`native_verdict:null` and a reason. The default timeout is 120 seconds.
This mode uses a dedicated single-flight connection; EOF, another request on
that connection, or server shutdown cancels it. Ordinary reporting remains an
immediate call. A timeout or cancelled reporter never means Allow.

Every `workspace_id` param accepts a workspace id **or its handle**: ids
match first, then handles (namespaces are disjoint — handles never contain
`_`). Handles are immutable slugs derived at create (`My API!!` →
`my-api`, collisions → `my-api-2`); renames change only the name.

Every state event appends to `$XDG_STATE_HOME/signaltty/audit.jsonl`
(`{seq, event, payload, at}`; rotation keeps one `.1` predecessor, 8 MiB threshold
each plus the crossing record). The audit record is synced before publication.
`event-sequence.json` reserves blocks of 4096 numbers using synced temporary write,
rename and parent-directory sync before issuance. Restart skips unused reservations;
PTY output and filtering also leave valid numeric holes. Journal/reservation write
failure stops the server; corrupt reservations fail startup. Snapshot durability is
separate. A legacy audit seeds the first reservation from its maximum retained number;
uncertain legacy/corrupt history requires recovery.

`subscribe {from_seq}` captures file/ring history, coverage and the handoff fence in
one Store read. The in-memory ring retains 1024 state events. Replay filters before
its 4096-event cap, deduplicates by `seq`, and returns in ascending order before live
events. The reply adds `replay {status, requested_after, retained_after, through,
returned, recovery?}`. `complete` proves all matching state events in
`(requested_after, through]`; numeric adjacency is not evidence of completeness.
`history_lost`, `truncated`, `unavailable` and `cursor_ahead` return
`subscribed:false`, zero replay frames and `recovery:snapshot_then_resubscribe`, then
close the connection. Retention metadata is persisted before discarding a generation and records expected
file lengths before publication. Missing or truncated generations invalidate proof;
uncertainty is checked before rotation and survives restart. Legacy journals remain
unavailable for disk completeness; the new runtime ring proves recent intervals.

On incomplete replay, open a fresh subscription **before** fetching current workspace
state and attaching panes. Queued state events through the handoff fence are suppressed.
Lag or out-of-order live delivery closes the stream; a disconnected client must refresh
state and reattach rather than assume no changes. GUI recovery keeps terminal widgets,
preserves PTY size and restores the current visible screen. It does not promise offline
scrollback. Mutating requests are never automatically resubmitted.

`pane.spawn`, `pane.split` and `pane.resume` include an optional
`integration {agent, status, changed, file?, notice?}` for direct supported-agent
launches. Status is `configured`, `disabled`, or `error`. Configuration precedes
process execution; a setup error does not prevent launch. Clients show `notice`
when present. `configured` is not proof of provider trust or event delivery.
Direct interactive local Codex execution adds native `--no-daemon` when a bounded
version probe confirms support; persisted argv remains unchanged. Unsupported
clients and explicit remote endpoints retain their command and return a disabled
integration notice (ADR-0014). Ordinary shell/arbitrary launches have no integration
result; manually typed Codex needs `--no-daemon`.

`pane.spawn` rejects an explicit tab from another workspace with `BAD_PARAMS`.
A failed process launch leaves no automatic tab or pane. Ownership validation,
process launch and pane publication complete before a concurrent close can
remove the target tab or workspace.

`tab.set_layout` requires every pane owned by the tab exactly once. Missing,
foreign or repeated pane IDs return `BAD_PARAMS` without mutation. Split ratios
must be finite and clamp to 0.05–0.95. Layout replacement arranges existing
panes; `pane.close` deletes one.

`tab.set_ratio` moves one divider: `path` holds 0 (first) / 1
(second) choices from the tab root (`[]` = root) and must resolve to
a `Split`, else `BAD_PARAMS`. `ratio` is clamped to 0.05–0.95 so a
client can never collapse a pane. The echo is `tab.updated`, like
`tab.set_layout`. GUIs persist dragged dividers through this method
rather than `tab.set_layout`: a targeted update cannot overwrite a
layout another client changed concurrently.

`workspace.file_diff` reads one literal UTF-8 filename relative to the checkout
root. It accepts workspace IDs or handles. Absolute, empty, NUL, `.`/`..` and
Git-private paths return `BAD_PARAMS`; wildcard-looking names remain literal.
Known tracked files, deletions and unignored untracked files are supported.
The comparison includes staged and unstaged edits against HEAD, or an empty
tree before the first commit. It uses three context lines, no rename detection,
and no external diff or textconv. It emits no event and issues no Git mutation command.
The application issues no edit/stage operation. As with the existing summary,
tracked reads retain canonical clean/process conversions from trusted local Git
configuration; those helpers can have their own effects. The Git process-group
deadline bounds them. Inherited `GIT_DIFF_OPTS` cannot override the context.

`content` is tagged by `kind`. Text contains `hunks`, `truncated` and optional
`notice`. Each hunk has `old_start`, `old_count`, `new_start`, `new_count`,
`heading` and `lines`. A line has literal `text`, optional `old_line`/`new_line`
numbers and kind `context`, `added`, `removed` or `no_newline`. Other content
kinds are `binary`, `unchanged` and `unavailable` with a literal `reason`.
Metadata-only changes and empty new files can have text with no hunks and a
notice. Untracked text is all additions and does not alter summary counts.

Preview bytes are capped at 512 KiB and lines at 10,000; stderr is bounded.
Truncated text retains complete lines and declares its incompleteness. Unsupported
content has an explicit unavailable state. The total server read deadline is
eight seconds (`TIMEOUT`); process/read failures use `IO_ERROR`. File acquisition
does not follow symlinks to read external content. See
[ADR-0016](adr/0016-file-diff-review.md) and the
[selected-file contract](../specs/014-file-diff-review/contracts/ipc.md).

## Events (all carry `seq`)

```text
workspace.created  workspace.updated  workspace.closed
worktree.changed
tab.created        tab.updated        tab.closed
pane.created       pane.updated       pane.exited        pane.closed
pane.resized       pty.data           pty.snapshot
agent.working      agent.blocked      agent.done         agent.failed
agent.idle         agent.unknown      agent.exited
attention.created  attention.updated  attention.cleared
decision.created   decision.answered  decision.cleared
notification.created
task.created       task.updated       task.result
git.branch_changed
server.will_shutdown
```

`worktree.changed {operation, path, workspace_id}` records checkout creation,
new workspace association or removal; `operation` is `create`, `open` or
`remove`, and `workspace_id` identifies the source workspace. Creation remains
observable if later convenience workspace binding fails and the checkout is
retained. Reopening the same associated workspace is a no-op.

`pty.data {pane_id, data_b64, output_offset}` streams only to connections
that ran `pane.attach` for that pane. `output_offset` is the cumulative end
byte offset of this output chunk. The attach reply carries the offset covered
by its snapshot, acquired atomically with terminal state. Streaming registration
precedes the final snapshot, and its response precedes subsequent stream events.
Clients discard or trim output already covered by that snapshot. A forward byte
gap requires reattach/reconnect before feeding more bytes. Offsets are
per-pane for the lifetime of the server and are not persisted. Repeated attach
on one connection registers one viewer; detach removes it and preserves the
process. Viewer registration survives a child exit and `pane.resume`, so
existing connections receive resumed output without attaching again.
All other events go to `subscribe`rs by glob match.

## Error codes

`BAD_PROTOCOL, UNKNOWN_METHOD, BAD_PARAMS, NO_SUCH_{SERVER,WORKSPACE,TAB,PANE},
NO_SUCH_TASK, NO_SUCH_DECISION, AGENT_BUSY, AGENT_NOT_READY, MERGE_CONFLICT,
PANE_EXITED, PANES_ALIVE, SPAWN_FAILED, IO_ERROR, TIMEOUT, IDENTITY_CHANGED,
RATE_LIMITED, FORBIDDEN, INTERNAL`.

`decision.created {pane_id, decision, prev?}` fires on set (a supersede folds
into one emit carrying the previous id). `decision.answered
{pane_id, decision_id, option_id}` fires on delivery. `decision.cleared
{pane_id, decision_id, reason}` fires when the bar drops unanswered
(`reason`: `attention_cleared | moved_on | pane_exited | native_cancelled`). A pending decision
clears on answer, when an agent transition clears its required attention or leaves `blocked`, or when the child exits. A still-unanswered decision at turn end (`Stop`/`SessionEnd`) is kept, never dropped. Clearing a decision never resumes a worker task: only an explicit answer or an accepted follow-up submit moves an `input_required` task back to `working`; timeout, disconnect, and turn-end drops park it in `input_required`. Reading,
focusing, `pane.mark_seen`, and default attach preserve the decision and gate.

## CLI mapping

Every CLI command is a thin wrapper over one method call with `--json`
passthrough of `result`. Examples: `signaltty new` → `workspace.create`
+ `pane.spawn`; `signaltty pane read` → `pane.read`; `signaltty wait`
→ `wait`; `signaltty notify` → `notify`. The CLI injects
`$SIGNALTTY_PANE` context for `notify`/`hook-event` when run inside a
pane (env set by the server at spawn).

Task orchestration CLI: `signaltty task start --repo … --objective …
… -- <argv…>` → `task.start` (the worker command is trailing args, not a
flag; returns `{task, pane}` at `pending`); `signaltty task get/list/wait
diff/file-diff/finish/cancel` → the matching `task.*` method (`wait`
defaults to `--until settled`; `finish` needs `--merge` or `--discard`);
`signaltty report --status … --summary … [--task …] [--pane …]` →
`task.report` (falls back to `$SIGNALTTY_TASK` / `$SIGNALTTY_PANE`);
`signaltty pane submit … --text …` → `pane.submit`;
`signaltty pane read … --mode rendered [--after-seq N]` → `pane.read`;
`signaltty pane spawn … [--parent-pane …] [--label …] [--relationship …]` →
`pane.spawn`; `signaltty attention [--limit N]` → `attention.pending`.
`signaltty schema` prints the live contract (same constants the router
dispatches on); a sync test proves every listed method dispatches.

`pane.resume` retains a directly selected path-qualified executable when its
basename matches the adapter's bare resume command, including older snapshots.
Explicit manifest resume paths and the retained configuration environment remain
authoritative. Launch options are not copied into resume arguments.

Relative executable paths supplied to `pane.spawn` or `pane.split` are anchored
to their launch directory before spawning and stored as absolute paths. Later
process directory changes therefore cannot redirect a session resume.

Legacy snapshots with a relative original executable keep the adapter command:
they do not persist a reliable initial directory to anchor that relative path.
