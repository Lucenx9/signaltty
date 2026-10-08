# Validation

Requires the development environment from docs/17-agent-development.md and Node.

Run `cargo test -p signaltty-integration opencode_plugin_ -- --nocapture`.
The tests install into a disposable home, import that installed source, invoke
V1 server() and V2 setup(), and capture reporter JSON. No paid provider calls.

Run `scripts/verify.sh full` before PR submission. Record the evidence directory
and independent review outcome here after completion.

## Red/green evidence (2026-10-08)

- `cargo test -p signaltty-integration opencode_plugin_v2_failure_reports_error_instead_of_done -- --nocapture`: failed before the fix with session.status/idle instead of session.error/message; passed after the translation fix.
- `cargo test -p signaltty-integration opencode_plugin_v1_creation_reports_session_info_identity -- --nocapture`: failed before the fix (zero reports); passed after reading info.id.
- `cargo test -p signaltty-integration opencode_plugin_ -- --nocapture`: six matching tests passed (five plugin behavior/export tests and the foreign-plugin preservation test), including both loaders, nested/legacy/missing errors, success/interruption, invalid V2 events, unmanaged no-op and unload cancellation.
- First full gate caught the missing Node fixture in the hermetic verification-runner tests. Updated those fixtures with the newly required executable; subsequent tooling checks passed.

Live paid provider sessions, desktop interaction and manual accessibility checks are not run. There are no GUI widget or style changes.

The first complete matrix attempt stopped at a pre-existing GTK focus assertion
(`theme_and_appearance_swapping_updates_window_classes`, grab_focus). An exact
isolated rerun passed without a GUI code change; the full matrix was restarted.
Grok also corrected the V2 fixture to use error.message (V1 uses error.data.message);
all six focused matching tests and formatting passed again.

Cairo focus diagnosis: the test window was 630px wide, with a collapsed hidden
sidebar. Waiting alone could not map its handle. The test explicitly reveals the
sidebar and waits for mapping before retaining the grab_focus assertion. The
exact cairo/X11 test then passed; no production GUI code changed.

Approval regressions (Sonnet 5.5 high via Claude Code):
- Red: `cargo test -p signaltty-server --lib answered_decision_resumes` reported Blocked instead of Working; `cargo test -p signaltty-server --test native_permissions installed_reporters` returned lifecycle blocked after an accepted Codex answer.
- Green: both commands passed after Store::answer_decision used the existing lifecycle transition; all seven native-permission tests and 67 server library tests passed.
- The existing TypeText delivery regression was updated to assert Working both in the answer response and pane.get, replacing its old Blocked expectation.

The next full run passed the focus test and exposed a wider-scene timeout in
worker_chip_sits_with_the_pane_title. `xvfb-run -a xdpyinfo` showed this host's
default was 640×480. The same exact worker test passed with
`xvfb-run -a -s '-screen 0 1920x1080x24'`. The shared GTK/benchmark runner now
specifies that display geometry rather than weakening scene assertions. All
temporary DEBUG-focus-023 / DEBUG-chip-023 probes were removed.

Independent review: native xAI Grok 4.7 high final scoped review found no blockers
for Store event order, shared native/TypeText lifecycle, OpenCode shapes and
GTK readiness. Claude Code Sonnet 5.5 high reviewed the OpenCode patch (no blockers),
then implemented the separately assigned approval transition with red/green proof.
Broader unverified audit candidates are triaged in research.md.

Final `scripts/verify.sh full`: PASSED. Evidence directory:
`target/verification/full-qtndbhuf/` (summary.json and individual logs).
All workspace/build/fmt/clippy/architecture/tooling checks, server QA, 22 isolated
GTK tests and refresh benchmark passed. Native light/dark scene renders were
inspected; this does not certify manual desktop/IME/accessibility or paid sessions.
