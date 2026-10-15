# Verification

```sh
SIGNALTTY_UI_EVIDENCE=/tmp/details-check GTK_A11Y=none xvfb-run -a -s '-screen 0 1600x1200x24' dbus-run-session -- cargo test -p signaltty-gui app::tests::breadcrumb_opens_the_workspace_details_card -- --exact --ignored --test-threads=1
```

Inspect details dark/light and long-data narrow/large/high-contrast/DejaVu/RTL captures. Test checks separate state/title, wrapping, complete tooltips, narrow content and action bounds, exact long-path and original clipboard bytes, Changes activation and the same mounted VTE. Full/delivery proof follows source freeze per docs17/18. Xvfb/classes do not certify real desktop screen-reader/IME behavior.
