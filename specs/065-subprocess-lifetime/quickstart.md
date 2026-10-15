# Verification

Run `cargo test -p signaltty-server --lib subprocess::tests`, the turn cancellation regression and existing worktrees tests. Run `cargo test -p signaltty-server --test file_diff --test turn_diff --test orchestrator_tasks --test orchestrator_finish --test orchestrator_pr --test worktrees` for callers. Finish with `scripts/verify.sh full` and delivery proof for `065-subprocess-lifetime`.
