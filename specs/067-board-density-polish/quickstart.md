# Verification

Run each display test in a separate process:

```sh
SIGNALTTY_UI_EVIDENCE=/tmp/board-polish-check GTK_A11Y=none xvfb-run -a -s '-screen 0 1600x1200x24' dbus-run-session -- cargo test -p signaltty-gui app::tests::task_board_shows_columns_and_navigates_to_pane -- --exact --ignored --test-threads=1
```

Repeat for task_board_fits_narrow_windows_and_reveals_last_column and open_task_board_preserves_identity_scroll_and_focus_on_updates. Final full and delivery commands follow docs17/18. Inspect screenshots; Xvfb does not certify desktop accessibility.
