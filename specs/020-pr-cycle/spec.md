# Feature specification: Pull-request cycle for orchestrated tasks

**Feature Branch**: `orch/pr-cycle`
**Status**: Implemented
**Spec**: 020 (follow-up to [018](../018-orchestrator/spec.md) and [019](../019-task-board/spec.md))
**ADR**: [0021](../../docs/adr/0021-task-pull-requests.md)

## Goal

A completed task can go out as a GitHub pull request instead of a local
merge. The task records the PR, a refresh reads its CI and review state,
and the board shows where each PR stands: still in review, ready to merge,
or blocked on you.

## User stories

1. **Open a PR** — Given a `completed` task with no disposition, when I run
   `signaltty task pr <id>`, the task branch is pushed to `origin` and a PR
   is opened against the recorded `target_branch`. The task gains
   `pr: {number, url, state: open}`. The worktree and pane stay, so the
   worker can address review.
2. **Refresh** — When I run `signaltty task pr-refresh [<id>]` (or open the
   board), each task with an open PR reads `state`, CI checks and review
   decision from GitHub and emits a task update.
3. **Board** — The board gains a **Ready to merge** column. A PR with
   failing checks or requested changes moves to **Needs you**; a merged or
   closed PR moves to **Done**. Cards show `#<number>` and the check state.
4. **Clean up** — After the PR is merged or closed, `task finish --discard`
   removes the worktree as today. `task finish --merge` on a task with an
   open PR is refused (`BAD_PARAMS`), so a branch is never merged twice.

## Model

`Task.pr: Option<TaskPr>` (serde-defaulted, old snapshots load):

| Field | Type | Notes |
|---|---|---|
| `number` | u64 | parsed from the URL `gh pr create` prints |
| `url` | string | |
| `state` | `open` \| `merged` \| `closed` | |
| `checks` | `none` \| `pending` \| `passing` \| `failing` | `none` until the first refresh |
| `review` | `none` \| `review_required` \| `approved` \| `changes_requested` | `none` = no review policy |
| `checked_at` | timestamp? | last successful refresh |

Checks roll up from `statusCheckRollup`: any check concluded
`FAILURE`/`ERROR`/`CANCELLED`/`TIMED_OUT`/`ACTION_REQUIRED`/`STARTUP_FAILURE`
(or status context `FAILURE`/`ERROR`) → `failing`; else any not completed
(or context `PENDING`/`EXPECTED`) → `pending`; else any → `passing`; empty →
`none`. The rollup is pure core logic with unit tests.

## IPC

| Method | Params | Result |
|---|---|---|
| `task.pr_open` | `{task_id, title?, body?, draft?}` | `{task}` |
| `task.pr_refresh` | `{task_id?}` | `{tasks}` (the refreshed ones) |

- `pr_open` requires `completed`, no disposition, no existing `pr`
  (`BAD_PARAMS` with `details.state` otherwise) and a recorded
  `target_branch`. Title defaults to the task label, body to the reported
  summary plus the objective.
- Both shell out (ADR-0021) with a 60 s deadline → `TIMEOUT`. A missing
  `gh` binary → `SPAWN_FAILED`. A non-zero exit → `IO_ERROR` with
  `details.stderr`. Nothing is recorded on failure; a retry pushes again.
- The state change goes through a `Store` transition that emits a task
  event (journaled like the other `task.*` events).

## Board columns (supersedes 019 rules where a PR exists)

A disposition `merged`/`discarded` or state `canceled` always wins over PR rules and maps to `Done`. For active tasks with a PR:

| Task | Column |
|---|---|
| Disposition `merged` / `discarded` or state `canceled` (wins over PR) | Done |
| `pr.state` merged or closed | Done |
| `pr.state` open, never refreshed (`checked_at == None`) | In review |
| `pr.state` open, checks `failing` or review `changes_requested` | Needs you |
| `pr.state` open, refreshed (`checked_at != None`), checks `passing`/`none`, review `approved`/`none` | Ready to merge |
| `pr.state` open, anything else | In review |
| no `pr` | 019 rules unchanged |

Opening the board fires one `task.pr_refresh` in the background; the board
re-renders from the task updates it emits.

## Out of scope

- Background polling, CI-failure auto-paste into the worker, merging the PR
  from signaltty (`gh pr merge`). Each is additive on `task.pr` and waits
  for a real need.
- Remotes other than `origin`, forges other than GitHub.
