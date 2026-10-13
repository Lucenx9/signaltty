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
