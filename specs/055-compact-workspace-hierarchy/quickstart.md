# Validation
Run isolated tests with `GTK_A11Y=none SIGNALTTY_UI_EVIDENCE=/tmp/compact-workspace-renders xvfb-run -a -s "-screen 0 1600x1200x24" dbus-run-session -- cargo test -p signaltty-gui <exact-name> -- --exact --ignored --test-threads=1`.
Check compact/selected rows, attention previews, chips, collapse, full-name/path accessibility and unchanged row/VTE identity. Render identical ten-workspace fixtures before/after, light/dark, narrow, large text/native high contrast and RTL. Inspect all captures.
Then `scripts/verify.sh full --output /tmp/compact-workspace-full`. Record explicit limits for real desktop screen-reader speech, on-screen keyboard, IME and paid providers. Independent review compares against the pinned base `d957d93` and spec055 before PR creation.
