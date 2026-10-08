# Verification and audit

Date: 2026-10-08. Base: `84d0307`. Runtime: Rust 1.94.0, Linux,
GTK 4.24.1, libadwaita 1.10.0, VTE 0.84.1.

## Reproductions and fixes

```sh
cargo test -p signaltty-server --test orchestrator_submit submit_does_not_send_delayed_enter_to_new_permission_request -- --exact
cargo test -p signaltty-server --test orchestrator_submit background_submit_permission_after_paste_keeps_task_recoverable -- --exact
cargo test -p signaltty-server --lib store::tests::clear_decision_never_resumes_task_for_non_answer_reasons -- --exact
cargo test -p signaltty-server --lib store::tests::background_ready_stays_working_when_pane_idle_or_live -- --exact
```

Before the fixes, the first real-PTY test captured an unwanted carriage return
(byte 13) after the permission hook. The background test found `failed` instead
of `input_required`. Store call-order regressions found `working` despite a
pending decision and stale `turn_ended_without_report` evidence.

After the fixes, the recorder contains only paste plus the drain marker, the
IPC refusal carries `delayed_enter` and `paste_delivered`, and the permission
remains unanswered. Background tasks remain recoverable and accept eventual
results. First-submit and follow-up commits preserve decision state and evidence;
answering the decision resumes work. All 13 submission and 18 Store tests pass.

## Repository checks

The final fast gate evidence is `target/verification/fast-ctv92ofe/summary.json`.
Full runs are recorded in `target/verification/full-wl83q0cc/summary.json` and
`target/verification/full-2kx1ruey/summary.json`. Workspace tests, server QA,
formatting, architecture, build and warning policy passed in those full runs.
The remaining display checks and benchmark are recorded in
`target/verification/orch-remaining-gtk/summary.json`.

Across the full run and remaining checks, 20 of 22 isolated GTK tests passed.
These unchanged tests failed locally:

- `app::tests::theme_and_appearance_swapping_updates_window_classes`,
  `app_tests.rs:936`, `handle.grab_focus()` failed. Reproduced alone, including
  with `GDK_DEBUG=no-portals`.
- `app::tests::worker_chip_sits_with_the_pane_title`, `app_tests.rs:1049`,
  UI operation timed out.

No GUI source is changed by this PR. The native palette light/dark captures
were inspected; focus, labels and contrast remain readable. The refresh
benchmark check passed. The full gate is **not green** on this host, so the
PR remains draft pending full verification in the supported environment.
Native accessibility, IME, folder choosers and real provider UI sessions were
not run. Xvfb does not certify those behaviors.

The first attempt used system Rust 1.99 and hit new Clippy warnings in unrelated
files. Installing and selecting the pinned Rust 1.94 removed them. No warning
baseline or unrelated source was changed.

## Independent review

Grok 4.7 used the native xAI provider. Its audit confirmed the Enter boundary,
identified premature background failure and the permission-before-task-commit
race. These were reproduced and corrected. Sonnet 5.5 high through Claude Code
approved both review rounds with no blocking findings. Test timing and error/doc
wording suggestions were applied. The final refinement refreshes evidence when
a decision arrives on an already interrupted task, with a red-green regression.

Gemini 3.6 Flash was attempted through Copilot; the provider rejected the model.
The user chose to continue with Grok and the parent review. OpenRouter was not
used. No temporary debug instrumentation remains.
