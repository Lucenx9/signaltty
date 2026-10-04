# 02 — Data Model (Server / Workspace / Tab / Pane)

```text
Server
└── Workspace "signaltty"  (repo / project / task)
    ├── Tab "agents"       (one layout)
    │   ├── Pane p1 (codex,  ~/code/foo)
    │   └── Pane p2 (claude, ~/code/foo)
    └── Tab "tests"        (another layout)
        └── Pane p3 (shell, ~/code/foo)
```

## Server

- `id`: opaque server id (stable per socket path).
- `socket_path`: `$XDG_RUNTIME_DIR/signaltty/signaltty.sock`.
- `state_dir`: `$XDG_STATE_HOME/signaltty` (`~/.local/state/signaltty`).
- Owns: workspaces, PTYs, event bus, persistence.

## Workspace

Usually one repo/project/task/investigation.

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `ws_…` |
| `name` | human label, editable, non-unique |
| `cwd` | default working directory for new panes |
| `repo_root`, `repo_name`, `branch`, `dirty` | cached Git context (see below) |
| `tabs` | ordered list of tab ids |
| `active_tab_id` | last focused tab (per last client; server keeps one canonical value) |
| `created_at`, `updated_at` | timestamps |

Git context is a lightweight cached enrichment, refreshed on
`pane.cwd` change, explicit `refresh`, and cheap filesystem events —
never a Git client, never aggressive polling.

Git worktree registrations are queried from Git rather than copied into the
snapshot. A workspace's canonical `cwd` associates it with a checkout;
`worktree.list` reports the open workspace IDs for each registered checkout.
Create and open produce ordinary workspaces with stable handles. Closing a
workspace preserves its checkout and branch. Removal is a separate operation
and refuses the main checkout, dirty or locked checkouts, and open references.

Selected-file diffs are transient read results, not workspace snapshot fields.
`signaltty_core::diff` defines typed text hunks with old/new line numbers and
explicit binary, unchanged and unavailable content. The server compares current
content against HEAD on demand; the GUI keeps selection and request generations
only for the open dialog. See [ADR-0016](adr/0016-file-diff-review.md).

## Tab

One named layout inside a workspace (`agents`, `shell`, `tests`, `logs`).

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `tab_…` |
| `workspace_id` | parent |
| `title` | human label |
| `layout` | binary split tree (see below) |
| `active_pane_id` | last focused pane in this tab |
| `created_at` | timestamp |

Layout is a binary tree:

```rust
enum Layout { Pane(PaneId), Split { dir: H|V, ratio: f32, first: Box<Layout>, second: Box<Layout> } }
```

The GUI renders this tree; the server validates and stores it. Each tab
layout must contain every pane owned by that tab exactly once. Whole-layout
updates arrange panes; deleting a pane requires `pane.close`. Ratios must be
finite and normalize to 0.05–0.95, matching individual divider updates.
Pane and tab workspace ownership must agree. Closing a tab or workspace
cleans every pane it owns, including panes hidden by legacy layouts.
Automatic tabs become visible only after their first process spawns.

Splitting a pane
replaces a `Pane` leaf with a `Split`, then rebalances the run: every
visual sibling in the same direction gets an equal share, so repeated
splits divide a tab into thirds, quarters, and so on instead of
halving the newest pane. Closing the last pane of a tab
closes the tab (configurable: keep empty tabs or not; default close).
Divider positions are plain ratios in the tree; clients move one
divider with `tab.set_ratio` (path of first/second choices from the
root, ratio clamped to 0.05–0.95), which persists like any other tab
change.

## Pane

One terminal/PTY + process. Survives client detach while the server lives.

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `pane_…` |
| `workspace_id`, `tab_id` | parents (pane can be moved) |
| `title` | from `OSC 0/1/2` title sequences, else `argv[0]` |
| `cwd` | launch cwd; refreshed from `/proc/<pid>/cwd` on Linux |
| `argv` | launch command (persisted; see restore policy in [09](09-persistence.md)) |
| `pty_size` | `{cols, rows}` last arbitrated size |
| `live` | `Live | Exited {code, at}` |
| `agent` | `AgentInfo` (below), all optional for plain shells |
| `lifecycle` / `attention` | see [03](03-lifecycle-attention.md) |
| `lifecycle_since` | when `lifecycle` last changed (optional; absent in old snapshots) |
| `attention_since` | when `attention` was last raised (optional; cleared with attention) |
| `last_run_secs` | length of the latest `working` stretch — GUI renders "Working for 2m…" → "Worked for 2m" |
| `last_message` | latest explicit notification / hook payload summary (never raw scrollback scrape as primary) |
| `pending_decision` | at most one structured decision `{id, prompt, options[{id, label}], answerable, received_at}` set by `hook-event` (directive 2); rendered as inline buttons iff `answerable`, cleared on answer / attention-clear / leaving `blocked` / exit |
| `scrollback_len` | bytes/lines retained (bounded ring) |
| `created_at`, `last_activity_at`, `last_seen_at` | timestamps |

## AgentInfo (per pane, adapter-owned)

| Field | Meaning |
|---|---|
| `kind` | `codex | claude | opencode | cursor | generic | none` |
| `agent_session_id` | native session id reported by adapter/hook (persisted) |
| `resume_argv` | adapter-built official resume command (persisted, never auto-run without consent) |
| `model`, `extra` | small adapter metadata map |

`agent_session_id` must come from hooks/APIs/env (`CLAUDE_CODE_SESSION_ID`,
`CODEX_THREAD_ID`, hook JSON payloads, OpenCode plugin events, Cursor
`hooks.json`), never from scraping terminal text when a better channel
exists.

Task lineage fields (all serde-defaulted, set at spawn): `parent_pane_id?`,
`root_pane_id?` (the parent's root, else the parent, else unset),
`label?`, `relationship?` (`fork`|`subagent`, default `subagent`),
`task_id?` (the owning task). At spawn the server injects
`SIGNALTTY_PARENT_PANE` (when a parent is set) and `SIGNALTTY_TASK`;
`$SIGNALTTY_PANE` still names the worker pane itself. Note: an unknown
`parent_pane_id` is stored as-is (no `NO_SUCH_PANE` check); the root is
left unset.

Native PermissionRequest decisions have a transient server response route to
their waiting reporter. The public decision is render data; its ID and live
route jointly identify the request that an answer can resolve. Route loss,
timeout, supersession, pane exit or session cancellation cannot grant access.
A saved decision cannot restore the reporter connection or its answerability.

## Task

One orchestrated worker: an agent pane running in its own git worktree cut
from a recorded base commit. Stored in `Store.tasks`, persisted as a
serde-defaulted `tasks` list on the snapshot (old snapshots load with no
tasks). IDs are opaque `task_…` / `tctx_…` strings.

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `task_…` |
| `context_id` | collaboration id (`tctx_…`); refinements are new tasks, same context |
| `parent_task_id?` | refinement link (currently always unset — reserved) |
| `pane_id?` | worker pane; absent before spawn / after pane removal |
| `parent_pane_id?`, `root_pane_id?` | lineage written at start |
| `relationship` | `fork`\|`subagent`, default `subagent` |
| `label` | human label |
| `contract` | objective (required, 1 byte … 32 KiB) + optional constraints, acceptance criteria list, output format. `task.start` validates this before creating a worktree. The composed worker prompt may be 32 KiB plus that task's fixed preamble; a larger body is refused and creates nothing |
| `agent?` | requested agent taxonomy kind |
| `source_repo` | repo the worktree was cut from; the merge target lives here |
| `target_branch?` | source repo's checked-out branch recorded at start (detached HEAD → unset; then finish needs an explicit `target_ref`) |
| `worktree_path`, `branch` | task-owned checkout + task branch |
| `preexisting_branch` | true when the branch already existed (never auto-deleted) |
| `base_ref`, `base_sha` | requested ref + resolved full SHA at start |
| `state` | lifecycle below |
| `result?` | structured handoff, set once by report (`status` completed\|failed\|rejected, `summary` 1 … 8 KiB, `artifacts` ≤ 32 `{name, path, version?}`, `evidence?`, `reported_at`) |
| `disposition` | `{outcome: none\|merged\|discarded, target_ref?, merged_sha?, branch_deleted?, at?}` — review outcome, not lifecycle |
| `status_reason?` | A2A-style evidence: `{stage, …}` for background/startup failures, `{reason: turn_ended_without_report, last_message?}`, `{reason: worker_silent, timeout_s}`, `{reason: decision_required, decision_id}` |
| `finish_error?` | conflict files, or `{cleanup_error}` when removal fails after a disposition is recorded. A later finish retries cleanup while the worktree path remains |
| `worker_pid?`, `worker_cmd?` | crash-vs-recycle evidence |
| `client_request_id?` | caller idempotency key from `task.start` (persisted) |
| `created_at`, `updated_at` | timestamps |

### TaskState

`pending → working ⇄ input_required → completed | failed | canceled | rejected`
(enforced by `can_transition_to`; terminal states are immutable).

| From | To |
|---|---|
| `pending` | `working`, `failed`, `canceled`, `completed`, `rejected` |
| `working` | `input_required`, `completed`, `failed`, `canceled`, `rejected` |
| `input_required` | `working`, `completed`, `failed`, `canceled`, `rejected` |

| Transition | Producer |
|---|---|
| `pending → working` | server-owned background step: waits for the worker pane to reach `idle`/`done` (default 30 s), writes the composed prompt, moves to `working` on write success |
| `pending → failed` | background failure with `{stage: ready_timeout\|submit_refused, …}` evidence (pane missing/exited/never idle, or submit write refused) |
| `* → canceled` (non-terminal) | `task.cancel`, or `finish --discard` from a non-terminal state |
| `working → input_required` | worker turn ended with no report (`turn_ended_without_report` + `last_message?`); silent-worker watchdog (`worker_silent` + `timeout_s`, default 600 s, `0` disables); pending decision on the worker pane (`decision_required` + `decision_id`) |
| `input_required → working` | accepted follow-up `pane.submit` on the worker pane (also clears `status_reason`) |
| `working/input_required/pending → completed/failed/rejected` | `task.report` status (report on a terminal task is refused) |
| `working/input_required/pending → failed` | worker pane death or restart recovery, with evidence, exactly once |

`task.start` takes an optional `client_request_id`: a retry with the same key
returns the existing task and pane (the key is persisted, so this also holds
across a server restart); without a key, or with a new one, every call
creates a new task.
`rejected` is worker-side ("finished but does not meet acceptance");
user-side refusal of finished work is `disposition: discarded` on a
`completed` task.

The submitted worker prompt is composed as: a preamble naming the task id,
branch and base SHA ("stay inside this worktree; commit your work; when
done or blocked run `signaltty report …` — it reads `$SIGNALTTY_TASK`"),
then `## Objective`, plus `## Constraints` / `## Acceptance Criteria` /
`## Expected Output Format` when the contract sets them.

### Read cursor (runtime only)

Per-pane monotonic `content_seq` over rendered lines (starts at 1, +1 per
rendered line). Ring capacity is 5000 rendered lines per pane. Cursors are
not persisted: after a restart any old cursor reads as dropped.

## Notification

| Field | Meaning |
|---|---|
| `id` | opaque |
| `pane_id`, `workspace_id` | routing; pane may be unknown for server-level notes |
| `title`, `body` | sanitized plain text (control chars stripped) |
| `severity` | `info | warning | error` |
| `source` | `osc9 | osc99 | osc777 | cli | hook:<agent> | adapter | system` |
| `created_at`, `read_at` | timestamps |

## IDs

UUIDv4 strings with a short type prefix for human greppability
(`ws_`, `tab_`, `pane_`, `notif_`). Prefix is cosmetic; treat as opaque.

`AgentInfo.config_env` retains only explicit `CLAUDE_CONFIG_DIR`, `CODEX_HOME`
and `OPENCODE_CONFIG_DIR` paths, normalized against launch cwd, for official
resume. Other environment variables and credentials are not retained.
