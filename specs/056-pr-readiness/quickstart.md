# Verify PR readiness

1. Run `cargo test -p signaltty-core workflow` to verify draft, conflicts, expiry, unknown facts, and legacy snapshots.
2. Run `cargo test -p signaltty-server --test orchestrator_pr` to verify typed refresh, errors, changed heads, and task read stages through isolated IPC.
3. Run `scripts/verify.sh full` for the required workspace and native GTK checks. Inspect its light/dark board renders.
4. With an existing managed task that owns a PR, run `signaltty task pr-refresh TASK_ID`, then `signaltty task get TASK_ID --json`. Inspect task.stage and task.pr facts. A draft or unknown merge state remains in_review.
5. Keep the board open past the five-minute observation lifetime. The card must leave Ready to merge on the next 30-second clock tick. A refresh failure shows refresh failed and preserves old facts.

The automated checks use fake gh and helper-owned sockets. Live GitHub mutations and paid provider turns are not required.
