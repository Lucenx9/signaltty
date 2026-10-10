# ADR-0029: Turn baselines as Git trees

**Status**: Accepted (2026-10-10) · **Spec**: `specs/041-turn-diff/` ·
**Supersedes**: ADR-0011 decision 2

## Context

The Changes panel compared the checkout with HEAD, so an agent that commits
during its turn erased its own work from review. docs/14 §6 asks for t3code's
per-turn diffs; ADR-0011 deferred them until the server records the worktree
at turn start.

## Decision

1. **A turn starts on `agent.working` unless `prev` is `blocked`.** Lifecycle is
   agent-agnostic (hooks and screen rules), and a permission prompt hands back
   to the same turn.
2. **The baseline is a Git tree of the whole checkout.** The server copies the
   index to a private file, runs `git add --all` and `git write-tree` against it
   through `GIT_INDEX_FILE`. Tracked and unignored untracked files are content;
   committing later does not hide them. Only Git objects are written: no ref,
   commit, index, HEAD or file of the user changes, so no hidden refs show up in
   `git log --all` or other tools.
3. **One baseline per workspace, runtime only.** The newest `agent.working`
   wins. Agents sharing a checkout share its latest turn. Unreferenced trees
   survive `git gc` for its prune expiry; a missing tree fails the read
   explicitly. A restart forgets baselines instead of persisting them.
4. **Turn reads compare two trees.** `scope: "turn"` captures a fresh tree and
   diffs baseline against it, for the summary and for one file.

## Consequences

- Review shows committed and uncommitted turn work and leaves out earlier dirt.
- A capture costs hashing the files changed since the index was last refreshed,
  bounded by a 20 s deadline; untracked build output that is not ignored is
  hashed too.
- Turn history, revert and restart persistence remain open; t3code's revert by
  hard reset stays out on purpose.
