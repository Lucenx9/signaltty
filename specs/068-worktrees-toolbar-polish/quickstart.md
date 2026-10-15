# Verification

```sh
SIGNALTTY_UI_EVIDENCE=/tmp/worktrees-toolbar-check GTK_A11Y=none xvfb-run -a -s '-screen 0 1600x1200x24' dbus-run-session -- cargo test -p signaltty-gui app::tests::navigation_palette_fast_enter_and_git_dialogs_use_native_controls -- --exact --ignored --test-threads=1
```

Inspect worktrees light/dark, narrow, large and row-focus captures. Assertions cover New text, no ellipsization, flat style and restored label; existing checks cover hit targets, native placement and operations. Run full and fresh delivery gate on frozen source as docs17/18 require. Xvfb does not certify real desktop accessibility.
