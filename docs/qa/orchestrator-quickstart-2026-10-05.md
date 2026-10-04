# Orchestrator quickstart — 2026-10-05

Feature: [018-orchestrator](../../specs/018-orchestrator/spec.md), task T031.
Run against `afdc8b5` on `main`.

## Manual quickstart

[`quickstart.sh`](orchestrator-quickstart-2026-10-05/quickstart.sh) replays
`specs/018-orchestrator/quickstart.md` sections 1–4 with fake `sh` workers
driven by hooks; output in
[`quickstart.log`](orchestrator-quickstart-2026-10-05/quickstart.log).

- Three `task start` calls return `pending` with `base_sha` and
  `target_branch: main`; each gets its own locked worktree.
- `SessionStart` + `UserPromptSubmit` move all three to `working`.
- Worker 2's `PermissionRequest` shows in `attention`; `decision answer`
  unblocks it.
- Worker 3's `Stop` without a report settles it at `input_required`;
  `pane submit` plus a new turn lets it report.
- All three settle `completed`; `attention` ends empty.
- `task diff` for task 1 lists only `file1.txt`.
- After a server restart task 1 is still `completed` with its result.
- `finish --merge --keep-branch` and `finish --merge` land two merge
  commits (the second deletes its branch); `finish --discard` removes the
  worktree and keeps the branch, per the contract. No task worktrees remain.

[`crash.sh`](orchestrator-quickstart-2026-10-05/crash.sh) kills a working
worker's shell: the task moves to `failed` with
`status_reason: {reason: pane_exited}`
([`crash.log`](orchestrator-quickstart-2026-10-05/crash.log)). The other
section 5 failure paths (dirty target, conflict, cap, stall) are covered by
`orchestrator_finish.rs`, `orchestrator_tasks.rs` and `orchestrator_submit.rs`.

## Verification

- `scripts/verify.sh full`: pass, including the 19 isolated GTK tests and
  the refresh benchmark.
- The E2E acceptance test in `orchestrator_e2e.rs` already runs un-ignored.
- The 018 diff was reviewed independently in its PR.
