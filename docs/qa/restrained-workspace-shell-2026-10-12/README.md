# Workspace shell, 2026-10-12

Production GTK captures from `scripts/verify.sh full --output /tmp/restrained-workspace-shell-verification-final` at clean `b15d7f7754b9cea2fe67575ababb88f48af5b78b`. All 47 steps passed, including 35 isolated GTK tests. Selected shell, tabs, menu, Changes, search, approval and enlarged/narrow evidence is retained here.

Additional high-contrast shell probe at the same revision:

```sh
GTK_A11Y=none ADW_DEBUG_HIGH_CONTRAST=1 SIGNALTTY_UI_EVIDENCE=/tmp/restrained-workspace-high-contrast-final xvfb-run -a dbus-run-session -- target/debug/deps/signaltty_gui-033da366973ed1d2 --exact app::tests::worker_chip_sits_with_the_pane_title --ignored --test-threads=1
```

Baseline Signal light/dark images are valid production-CSS shell captures from feature050's clean `8bf1d3e` full run. The older search fixture did not load production CSS and is excluded from comparisons. New captures explicitly load it. Approval scenes use the existing standalone pane fixture; full shell scenes show the app icon resource context.

Manual desktop screen reader, IME, on-screen keyboard, chooser, notification and paid-provider integration were not run. Xvfb/high-contrast evidence does not certify those desktop paths.
