# Tasks: Reviewed terminal attention

## Setup

- [x] T001 Record spec and plan in specs/024-reviewed-terminal-attention/.

## User stories 1-3

- [x] T002 [US1-3] Add Store unit tests in crates/signaltty-server/src/store.rs: reviewed done/failed exit stays none; working, idle, unreviewed and decision-bearing exits still unread.
- [x] T003 [US1-3] Add real hook-IPC regressions in crates/signaltty-server/tests/integration.rs (Claude, Codex, Cursor SessionEnd after Stop+mark_seen; PTY exit; failed preserved; first completion; fresh turn; permission gate) and run them red.
- [x] T004 [US1-3] Implement the snapshot predicate in crates/signaltty-server/src/store.rs; use it in set_exited and suppress trailing terminal session-end decisions in the router.
- [x] T005 Run the same tests green plus the existing store/integration suites, fmt and clippy.

## Documentation

- [x] T006 Update docs/03-lifecycle-attention.md and docs/07-agents.md.

## Evidence

Red (production unchanged, 2026-10-08): `cargo test -p signaltty-server --lib reviewed_terminal` failed `Claude Done: left Unread, right None`; `--test integration` `reviewed_terminal_session_end…` (unread re-raised), `session_end_never_rewrites_a_failed_outcome` (`failed` became `done`), `reviewed_terminal_pty_exit…` and `session_end_and_exit_still_alert…` (last message replaced by "session ended") failed. Guards `exit_still_alerts…`, `fresh_turn_after_review…`, `session_end_keeps_unanswered_permission_gate` were already green by design.
Green: same commands pass; mutating only the `set_exited` guard re-reds the PTY-exit test. Wider: `cargo test -p signaltty-server -p signaltty-agent -p signaltty-core`, `cargo fmt --check`, `cargo clippy -p signaltty-server --all-targets` clean. `scripts/verify.sh full` PASSED after rebasing onto main: target/verification/full-o60xfo1i/summary.json, 25 isolated GTK tests, server QA and refresh benchmark. Dark native scene inspected. Independent Gemini review pending; no paid sessions or manual desktop accessibility tested.

## CI isolation follow-up

CI run 37827483980 failed existing orchestrator_pr tests: two TempGitRepo
instances shared a directory (origin already exists / HEAD missing). The XOR
namespace generator can cancel an advancing sequence against an advancing clock.
A deterministic test of `(pid=7, nanos=100, sequence=0)` versus
`(pid=7, nanos=101, sequence=1)` reproduces identical names. Namespace components
are now concatenated independently, using full epoch nanoseconds.

Codex P2 review: failed worker + SessionEnd with live shell previously left its
task Working indefinitely. Real worker regression failed Working vs InputRequired;
turn-end settlement now runs independently of the suppressed pane transition,
preserving Failed/Error and the failure message.

Follow-up full gate PASSED: target/verification/full-kzhr_u4_/summary.json, 25 isolated GTK tests, server QA and benchmark. Grok approved namespace isolation; worker settlement review is pending.
