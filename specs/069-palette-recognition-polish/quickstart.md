# Verification

Run each GTK test in a fresh display/session, substituting its exact name:

```sh
SIGNALTTY_UI_EVIDENCE=/tmp/palette-check GTK_A11Y=none xvfb-run -a -s '-screen 0 1600x1200x24' dbus-run-session -- cargo test -p signaltty-gui app::tests::palette_empty_results_and_shortcuts_fit_narrow_appearances -- --exact --ignored --test-threads=1
```

Also run navigation_palette_fast_enter_and_git_dialogs_use_native_controls, palette_keyboard_selection_stays_visible_while_search_keeps_focus and owned_icon_roles_resolve_and_render_without_external_themes. Inspect light/dark, 360px, enlarged/high-contrast and RTL captures. Icon resources must be registered in standalone fixtures as in production startup; set toolkit default RTL as well as parent direction for separate dialog roots. Freeze all source before full/delivery verification, per docs17/18. Native roles and snapshots do not certify real screen-reader/IME operation.
