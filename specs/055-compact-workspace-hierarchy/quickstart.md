# Validation

Use a fresh Xvfb display and D-Bus session per exact test, with `GTK_A11Y=none SIGNALTTY_UI_EVIDENCE=/tmp/compact-workspace-renders xvfb-run -a -s "-screen 0 1600x1200x24" dbus-run-session -- cargo test -p signaltty-gui <exact-name> -- --exact --ignored --test-threads=1`.

Exact names:
- `sidebar::tests::workspace_previews_follow_selection_without_hiding_attention_or_tasks`
- `sidebar::tests::row_refresh_keeps_status_and_metadata_identity_when_headline_changes`
- `app::tests::shell_empty_states_explain_actions_and_fit_large_text`
- `app::tests::workspace_header_context_survives_shell_agent_and_empty_transitions`

Check compact/selected rows, attention previews, chips, collapse, complete identity tooltips/accessibility and unchanged row/VTE identity. The shell fixture renders the same ten populated workspaces before/after, light/dark, 200px enlarged text and RTL. Repeat the shell test with `ADW_DEBUG_HIGH_CONTRAST=1`; the test asserts native StyleManager high contrast. Inspect the captures.

Run `scripts/verify.sh full --output /absolute/fresh/evidence-directory`. On the contended four-CPU development host the recorded final gate uses `RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/compact-workspace-full-serial-20261013`, serializing test cases while retaining every check and timeout. The QA directory preserves the failed parallel attempts and isolated retry.

Record explicit limits for real desktop screen-reader speech, on-screen keyboard, IME, choosers, notification clicks and paid providers. Independent review compares against pinned base `d957d93` and spec055 before PR delivery.
