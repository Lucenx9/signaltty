# Tasks: Resume Retains Selected Executable Path

## Setup

- [x] T001 Create specification and implementation plan in `specs/025-resume-selected-executable/`.

## Tests (Red Phase)

- [x] T002 Add unit test suite in `crates/signaltty-agent/src/resume.rs` covering all matrix cases (5 built-in adapters, relative paths, empty inputs, bare commands, promoted shells, manifest explicit paths, suffix flags, wrapper basenames).
- [x] T003 Add real IPC integration tests in `crates/signaltty-server/tests/integration.rs` (sibling without manifest, hook-event writer, stale bare resume argv repair).
- [x] T004 Run tests and record RED failure output in tasks evidence.

## Implementation (Green Phase)

- [x] T005 Implement `resolve_resume_argv` in `crates/signaltty-agent/src/resume.rs` and export from `signaltty-agent`.
- [x] T006 Wire `resolve_resume_argv` into `h_hook_event`, `h_pane_report_session`, and `h_pane_resume` in `crates/signaltty-server/src/router.rs`.
- [x] T007 Run tests and verify GREEN.

## Documentation & Quality

- [x] T008 Update `docs/07-agents.md` and `docs/08-ipc.md`.
- [x] T009 Run focused formatting and clippy (`cargo fmt --check`, `cargo clippy -p signaltty-agent -p signaltty-server --all-targets`).
- [x] T010 Final summary report.

## Evidence

(To be recorded during T004 and T007)

Red evidence (parent completed after Gemini internal error):
`cargo test -p signaltty-agent resume::tests` failed 3 path-preservation tests;
`cargo test -p signaltty-server --test integration report_session_builds_resume_retains_selected_executable_without_manifest` failed bare codex versus selected absolute fixture.
Green: all 8 helper tests and both selected-executable IPC writer tests passed.
Stale snapshot fixture now shuts the server down before editing its snapshot,
so graceful restart cannot overwrite the intended old bare vector.

Stale-snapshot repair test passed; all three IPC regressions are green.
Full gate and independent native xAI Grok review are in progress.

Full verification passed: target/verification/full-d_404mi0/summary.json, 25 GTK tests, workspace tests/clippy/formatting, server QA and benchmark. Independent Grok review pending.

Combined branch gate passed after parent PR45 follow-ups: target/verification/full-sgeu79m7/summary.json (25 GTK tests). Parent PR45 subsequently merged; native xAI Grok review is still pending.

Grok final review found relative launch paths could drift after cwd refresh. Real IPC regression observed chdir, then failed on non-absolute resume argv after 10s. Anchor launch argv for spawn/split; replace snapshot-edit delay with owned-child exit fence. Green and revised full gate pending.

Green: both real chdir regressions (spawn/split), plus both session reporters, passed; selected executable resumes from the changed cwd. Independent Grok r9 and revised full gate pending.

Codex P1: legacy snapshots bypass launch anchoring. Red helper matrix and restored-snapshot report-session test both substituted ./bin/codex incorrectly; restrict original selection to absolute paths, preserving adapter command for legacy relative argv. Revised gate pending.

Green: all8 helper tests and restored legacy-relative snapshot IPC test pass. Both fresh relative launch paths remain anchored by spawn/split. Final full gate and native xAI Grok r10 pending.

Final code verification passed on ae621dd: target/verification/full-njr2rvuv/summary.json, 26 isolated GTK tests, workspace/clippy/formatting, server QA and refresh benchmark. CI rust and minimum-rust passed on ae621dd. Native xAI Grok4.7 r10 APPROVE on ae621dd (read-only, no cargo); remaining spec/plan wording aligned in this documentation-only follow-up. No paid provider sessions or manual desktop accessibility claimed.
