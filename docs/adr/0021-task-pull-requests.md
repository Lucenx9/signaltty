# ADR-0021: Task pull requests through the `gh` CLI

**Status**: Accepted, 2026-10-05. **Spec**: [020-pr-cycle](../../specs/020-pr-cycle/spec.md).

## Context

ADR-0020 deferred the PR/CI cycle: a finished task could only merge locally.
Opening a PR and reading CI and review state needs GitHub credentials. The
server runs as the user and already shells out to `git`.

## Decision

- The server runs the user's `gh` CLI (`gh pr create`, `gh pr view --json`)
  and `git push -u origin <branch>` from the task worktree. signaltty stores
  no token and adds no HTTP client: auth, hosts and enterprise config are
  whatever `gh auth` already has.
- Each call runs off the async runtime with a 60 s deadline. Failure records
  nothing; `gh`'s stderr comes back in `details.stderr`.
- A PR is not a disposition. `Task.pr` sits beside `disposition`, so the
  existing finish paths keep their meaning: discard cleans up after the PR
  lands, and a local merge is refused while a PR is open.
- State is read on demand (`task.pr_refresh`, and when the board opens), not
  polled. Polling, CI auto-paste and `gh pr merge` are additive later.

## Consequences

`gh` becomes an optional runtime dependency: only `task.pr_*` need it, and
its absence is a clean `SPAWN_FAILED`. Tests use a fake `gh` script on the
server's `PATH` and a local bare repository as `origin`.
