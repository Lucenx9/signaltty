# Tasks: Newline-safe worktree registration

- [x] T001 [FR-001] Reproduce a registered newline path as unregistered in crates/signaltty-server/src/git.rs.
- [x] T002 [FR-001] [FR-003] Parse NUL-delimited worktree porcelain in `is_worktree_registered`, comparing non-UTF-8 fields lossily.
- [x] T003 [FR-002] Keep the existing add/remove registration guard and fail-closed Git errors.
- [x] T004 [FR-001] [FR-002] [FR-003] Complete review and full verification before publishing.
