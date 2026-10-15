# Verification: Newline-safe worktree registration

## Diagnosis

`cargo test -p signaltty-server --lib git::tests::newline_in_a_registered_worktree_path_stays_registered -- --exact` failed before the fix: `git worktree add` succeeded and the assertion `is_worktree_registered` panicked with "registered path containing a newline must not look unregistered". The same command passed after the parser change, with `worktree_registration_guard` still passing.

Ranked hypotheses:

1. Line-oriented porcelain splits the path at the newline, so the guard compares a prefix. Prediction: NUL fields make the same test pass. Confirmed.
2. Canonicalization fails for a newline path, so only a string compare of Git's stored path would pass. Not required: the query path matched the NUL field directly.
3. Git refuses to register a newline path. Falsified: `worktree add` succeeded and the failure was the registration assertion.

The correct cause is hypothesis 1. `git worktree list --porcelain` without `-z` emits the newline inside the path as a line break. `lines()` therefore never sees the full path, the guard returns false, and `remove_worktree_checkout` may `remove_dir_all` a checkout Git still owns when `git worktree remove` fails.

Review of `3b74908` found a second hole in the strict UTF-8 field parse. Git stores the canonical bytes when a UTF-8 path is a symlink, and those bytes may not be UTF-8. Skipping that field made `utf8_symlink_to_a_non_utf8_worktree_stays_registered` fail. Lossy comparison of each NUL field matches the caller's lossy canonical path. An unrelated non-UTF-8 worktree does not mark every path registered. The same test passes after that change.

## Delivery proof

Run `scripts/verify.sh full --output target/verification/worktree-newline-registration-full` against final staged content, then `scripts/agent.sh check --phase delivery --feature 066-worktree-newline-registration --evidence target/verification/worktree-newline-registration-full`. The report records the result. GitHub must pass the latest PR workflow's workflow-lint, rust and minimum-rust jobs for the reviewed head and base before merge.

No GUI behavior changed. A later cleanup can share this NUL parse with `worktrees::registered`; that refactor is outside this fix.
