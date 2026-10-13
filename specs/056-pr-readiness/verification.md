# PR readiness verification, 2026-10-13

## Scope

Branch `orch/pr-readiness` in the isolated `pr-readiness` checkout, based on `d957d93f128686b599a9cce6573fb62e9b996e3f`. The feature directory is 056 because newer parallel feature directories were already present upstream. No application state or default socket was used.

## Behavioral evidence

TDD reproduced legacy facts yielding Ready to merge, missing PR facts, malformed nested check data, delayed batch invalidation, stale responses after a newer failure, and loss of the latest successful facts after a failed overlapping attempt. Focused core, server IPC, and CLI regressions pass after their fixes.

The installed gh 2.86.0 exporter can serialize an empty rollup as null. The IPC regression confirms null is accepted as no checks, while an absent field is rejected. No authenticated coding-agent turn or remote merge was performed.

## Native GTK evidence

The ignored `task_board_shows_columns_and_navigates_to_pane` test passes in isolated Xvfb/D-Bus. Each task fixture has a distinct pane. It verifies ready, draft, and failed-refresh states, then expires a ready observation without a server event and verifies that its card moves to In review.

| Before | After | Why |
| --- | --- | --- |
| PR notes followed long agent/branch metadata and disappeared behind ellipsis. | PR notes lead the same metadata line. | Draft, refresh failed, and refresh needed remain readable in narrow cards. |

The ten headless board tests and the native scene were rerun after the metadata ordering change. The light/dark and expired-state images below were inspected. Column counts, task labels, and leading PR notes are legible. Keyboard/focus behavior remains covered by the existing isolated suite.

- [Dark board](evidence/board-dark.png)
- [Light board](evidence/board-light.png)
- [Expired PR](evidence/board-pr-expired.png)

## Independent review

Standards and Spec reviews ran separately. Four concrete findings were reproduced and resolved: unknown check normalization, malformed check shape fallback, delayed batch failure publication, and stale final batch views. Final reviews reported no remaining findings. A final compatibility review confirmed nullable rollup decoding without accepting missing fields.

## Required gate

`RUST_TEST_THREADS=1 scripts/verify.sh full` passed. Its unique local evidence directory is `target/verification/full-4goonb32`; the [committed result summary](evidence/verification.json) records the actual base revision, dirty state, tool versions, and step outcomes. All 41 ignored GTK tests and the refresh benchmark passed. The final metadata-ordering change also passed the ten headless board tests, an isolated native scene with inspected light/dark captures, `cargo fmt --check`, and `cargo clippy -p signaltty-gui --all-targets --locked`.

Live GitHub branch-policy coverage, compositor accessibility, IME, and paid provider sessions are not certified by these fixtures. Readiness remains an advisory cached observation with a five-minute lifetime.

## Bot review follow-up

Codex found that a long batch reused its start time for later PR observations. The IPC regression failed before the fix because the second PR's attempt time equaled the first observation time. Each PR now records its own attempt time, and `checked_at` records successful completion. A paused-command regression verifies that completion time follows release of the command. All seven `orchestrator_pr` tests pass, including concurrent refresh ordering and preservation of newer failures.

CodeRabbit found a misplaced brace in the Task field table. The documented observation fields now belong to `pr`, matching `TaskPr` serialization. Its generic docstring-coverage warning is not a repository requirement. Local Clippy passes. Copilot did not review because its quota was exhausted.

The first follow-up full run (`target/verification/full-w71tr0_v`) passed workspace tests and server QA, then stopped at the existing native-dialog test's narrow-breakpoint assertion. The same test passed in isolation with the runner's X11, Cairo, and screenshot settings, without a code change. GitHub CI reruns full verification on the follow-up commit before merge.
