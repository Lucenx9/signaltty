# ADR-0039: Shared command lifetime with operation-owned policy

Status: accepted, 2026-10-15. Spec: [065-subprocess-lifetime](../../specs/065-subprocess-lifetime/spec.md).

## Context

Selected-file diffs, turn capture, worktree operations and task commands each
managed child lifetime. Task commands imported a process-group guard from the
file-diff module and changed its internal field. Turn capture killed only Git
itself on cancellation. A real filter test reproduced a descendant continuing
to write after its capture was canceled.

## Decision

Use a private server `subprocess` module for prepared command execution. It owns
null stdin, concurrent stdout/stderr capture, a new process group, cancellation,
stream-limit termination and direct-child reaping. Callers own arguments,
environment, optional deadlines, optional stream limits and error interpretation.
No new limits are imposed on previously uncapped operations.

Drain both pipes before polling the child wait. An exited but unreaped leader
keeps its process-group identifier reserved while descendants may retain pipes.
On completion, the child no longer has an ID and the guard sends no signal.
On timeout or read failure, kill the group and await the direct child. If the
caller drops the future, kill the group synchronously and let Tokio's
kill-on-drop/orphan reaper finish direct-child cleanup. Cleanup does not need a
detached supervisor task or a cancellation channel.

Keep file-diff's eight-second outer operation deadline and existing preview and
diagnostic caps. Keep turn/worktree per-command deadlines and task command
deadlines. Git conversions, private-index capture and merge recovery remain
the caller's responsibility, as specified by ADR-0016, ADR-0029 and ADR-0020.

## Consequences

Callers no longer manipulate process groups or expose guards across modules.
Real executable tests cover both pipes, exit and spawn failures, stream limits,
deadlines and cancellation with live or exited leaders. Existing operation tests
continue to prove Git behavior. The helper does not manage PTYs, workspace-title
processes or all synchronous Git commands. Descendants that deliberately leave
their process group are outside this existing cleanup contract.
