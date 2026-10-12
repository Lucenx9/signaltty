# Reliable workspace interactions, 2026-10-12

Production code at clean `61f3d9849134044b304e1ef2b6b1a93394443b35` passed `scripts/verify.sh full --output /tmp/reliable-workspace-interactions-verification-final`: 50 steps, 38 isolated GTK tests, workspace tests, server QA and refresh benchmark. Summary retained here. The earlier b161351 full run also passed; only QA adapter changes followed it.

## UX acceptance

| Before | After | Why |
| --- | --- | --- |
| Live panes/tabs stopped immediately; tabs disappeared before server reply | Cancel-default confirmation for live processes; reply-driven reconcile | Protect work and preserve targets/widgets on cancellation or failure |
| New Workspace dismissed on submit | Native action dialog stays mounted with pending feedback and explicit Retry | Keep choices and explain async results |
| Partial creation left no recovery path | Retain created workspace id, lock its folder/name, allow corrected command retry | Avoid duplicate workspaces |
| Invalid fields only disabled Create | Wrapped field-local explanations and accessible descriptions | Explain how to proceed |
| Board/Changes disappeared below 760sp | Labeled pinned sidebar controls replace hidden header controls | Recognition in compact layouts |

All 12 selected images are native GTK renders from supplementary passing runs at the same clean `61f3d98` code. These runs use the supported reduced-motion setting to avoid sampling a partially animated dialog. Default-motion runs remain the full behavioral gate; transition frames are excluded from selected visual proof.

Reproduce the selected scenes:

```sh
mkdir -p /tmp/signaltty-ux-visual-config/gtk-4.0
cat > /tmp/signaltty-ux-visual-config/gtk-4.0/settings.ini <<'SETTINGS'
[Settings]
gtk-enable-animations=false
SETTINGS
GTK_A11Y=none GDK_BACKEND=x11 GDK_DEBUG=no-portals XDG_CONFIG_HOME=/tmp/signaltty-ux-visual-config SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-ux-visual xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui app::tests::workspace_creation_retains_draft_and_retries_only_the_failed_command -- --exact --ignored --test-threads=1
```

Repeat separately for `app::tests::terminal_closures_confirm_live_targets_and_preserve_failed_tabs` and `app::tests::visible_workspace_tools_open_palette_and_board_without_main_menu`. The creation fixture holds its first reply until pending scenes are captured; retry uses deterministic before-create and startup failures. The enlarged error scene explicitly scrolls to the explanatory label and checks its bounds. Compact scenes use 360px and Sans 18 with the application's high-contrast class. Settings and sockets are isolated; no user preferences or real projects are mutated.

## Review and proof limits

Independent Standards and Spec/UX reviews of the final runtime/QA diff found zero issues. Native light/dark, enlarged error, pending/retry and compact controls were inspected. Claims are heuristic/behavioral acceptance, not a usability study.

An optional headless AT-SPI probe exposed the pre-existing `Accessible object is not iterable` incompatibility; the QA adapter now uses the documented child count/index APIs. Subsequent attempts stopped before app verification with `Failed to open display`. The updated desktop cancellation/confirmation probe is not claimed as passed. Screen-reader interaction/order, OSK, IME, real folder chooser, notification clicks, compositor integration and paid-provider sessions remain not run. The class-based high-contrast scene does not certify a desktop high-contrast theme.
