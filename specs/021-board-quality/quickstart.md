# Validate the board correction

Run `scripts/verify.sh doctor`, then:

```sh
GTK_A11Y=none SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-board-quality xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui app::tests::task_board_fits_narrow_windows_and_reveals_last_column -- --exact --ignored --test-threads=1
scripts/verify.sh full
```

Inspect the exported light, dark and enlarged high-contrast images. The native window stays 360px wide; horizontal scrolling and keyboard focus reach Done. Activating its card returns `pane_done`.

Screen-reader order and on-screen keyboard input require a desktop manual check and are not certified by the isolated GTK test.
