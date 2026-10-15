# Plan: Newline-safe worktree registration

## Context and constitution

Use Rust 2021 and the existing leftover-directory guard in `remove_worktree_checkout`. This fixes a parser bug. It does not change cleanup policy or add an architectural decision. Read docs/17, docs/19 and the canonical verification skill. The failing test precedes the fix.

## Implementation

Read `git worktree list --porcelain -z` and compare `worktree ` fields split on NUL. Keep fail-closed behavior when Git fails. Keep matching the query path or its canonical path. Do not delete a registered checkout from this change.

## Verification

Add `newline_in_a_registered_worktree_path_stays_registered` beside the existing registration guard. It must fail before the parser change and pass after. Re-run the existing guard. Run `scripts/verify.sh full` on staged final source and the feature delivery gate.
