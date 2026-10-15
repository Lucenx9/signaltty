# Specification: Newline-safe worktree registration

**Created**: 2026-10-15
**Status**: Clarified
**Input**: Continue finding repository bugs, publish confirmed fixes and merge after checks.

## User story

Task cleanup may delete a leftover directory only after Git says that path is not a registered worktree. A registered checkout whose path contains a newline must still count as registered, so a failed `git worktree remove` cannot be followed by `remove_dir_all`.

## Requirements

- **FR-001**: `is_worktree_registered` reports a checkout registered when `git worktree list --porcelain -z` lists that path, including when the path contains a newline.
- **FR-002**: A path Git does not list stays unregistered. When Git itself fails, the path stays registered (fail closed).
- **FR-003**: The comparison still accepts the caller's path or its canonical form. No IPC, persisted data, or cleanup policy change.

## Acceptance scenarios

1. A worktree added at an absolute path containing a newline is registered.
2. The existing guard still reports a normal path unregistered before add, registered after add, and unregistered after remove.
3. A failed Git listing does not become "not registered".

## Success criteria

The server unit test creates a real Git worktree whose path contains a newline and requires `is_worktree_registered` to return true. Existing registration tests pass. Full repository verification passes.

## Clarification

No user questions are required. The leftover-directory guard already refuses to delete a checkout Git still claims. The defect is the line-oriented parser hiding that claim. Sharing one porcelain parser with worktree listing is a later cleanup, not this correction.
