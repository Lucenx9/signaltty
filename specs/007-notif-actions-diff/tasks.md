# Tasks: Notification Actions + Worktree Diff

 - [x] T001 [US1] `notif.rs`: `mark-read` action + `action_event()` pure fn
  + unit tests; `actor.rs` `UiEvent::MarkSeen`; `app.rs` arm
  (`pane.mark_seen` + refresh).
 - [x] T002 [US2] `git.rs` `git_diff` + fixture unit test (tracked,
  untracked, binary, rename-as-is, non-repo None).
 - [x] T003 [US2] Proto `WORKSPACE_DIFF` (count 34→35) + router
  `h_workspace_diff` (handle-or-id, non-repo → `BAD_PARAMS`) +
  integration tests.
 - [x] T004 [US2] CLI `workspace diff` (human + `--json`) + cli.rs test.
 - [x] T005 Docs: `docs/08-ipc.md` row + `docs/adr/0011-diff-and-deferrals.md`.
 - [x] T006 Gates: fmt, clippy (zero new), full tests; commits.
