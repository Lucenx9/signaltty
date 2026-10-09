# Local workflows

## Sub-features

Command/workspace palette, workspace rename, literal terminal search, pane zoom,
worktree create/open/remove, working-tree change summaries and per-file diff review.

## How to get to it (user POV)

Use Ctrl+Shift+P for the palette, Ctrl+Shift+R to rename, Ctrl+Shift+F to search
and Ctrl+Shift+Z to zoom. The window menu also exposes these commands, Worktrees
and Show Changes. Worktree creation opens a shell workspace. Closing that
workspace preserves its checkout; removal is a separate confirmation.
In Show Changes, activate a file to read its numbered patch. Back returns to the
preserved file list; Refresh reads current changes again.

## Driving it with GTK and real Git

Run `cargo build --workspace` then `cargo test -p signaltty-server --test worktrees`.
The suite drives actual IPC and the CLI executable against isolated repositories:
checkout isolation/reuse, literal paths, restore, branch retention, dirty/main/locked
refusal, background-process cwd checks and removal racing ordinary creation/launch.

Run each display test in its own process with `GTK_A11Y=none dbus-run-session --
cargo test -p signaltty-gui <name> -- --exact --ignored --test-threads=1`:

- `app::tests::open_task_board_preserves_identity_scroll_and_focus_on_updates`
- `app::tests::task_board_live_activation_and_neighbor_fallback_use_current_rows`
  covers current pane activation, same-column fallback and surviving reading anchors
  when removals force GTK to clamp a nearly-bottom viewport.
- `app::tests::task_board_explains_truncated_done_history`
- `app::tests::workspace_header_context_survives_shell_agent_and_empty_transitions`
- `app::tests::navigation_palette_fast_enter_and_git_dialogs_use_native_controls`
- `app::tests::palette_keyboard_selection_stays_visible_while_search_keeps_focus`
- `app::tests::palette_empty_results_and_shortcuts_fit_narrow_appearances`
- `app::tests::file_diff_reader_uses_native_numbered_selectable_controls`
- `app::tests::pane_zoom_keeps_hidden_terminals_and_restores_latest_ratios`
- `terminal::tests::terminal_search_is_literal_and_keeps_the_terminal`

Set `SIGNALTTY_UI_EVIDENCE=/absolute/evidence/path` for the navigation
test to export actual GTK window renders of palette/worktrees/changes in light
and dark themes. Terminal search and zoom tests export their scenes to the same
path when supported. Inspect every saved image before accepting it.

Observe shell branch/path fallback, preserved agent summary, cleared context and
separator after the last workspace, and header controls inside 360px in both
appearances with enlarged text. The header test exports native scenes through
`SIGNALTTY_UI_EVIDENCE`.

Observe selected-result visibility through long-list arrow navigation and filtering
after scrolling, mapped no-match guidance and recovery, readable shortcut subtitles at
360px with enlarged text, immediate typing+Enter navigation, blank rename refusal,
stable handles,
unchanged VTE objects, continued hidden output, unchanged server ratios, literal
file labels and binary/untracked indicators. Actor tests also prove slow worktree
requests keep the normal control and PTY stream available and are never retried.

## Gotchas

GTK dialog tests inject replies at the production IPC actor seam to exercise
widgets without mutating real checkouts. Real filesystem effects are established
separately by the server/CLI suite. The diff is against HEAD, not per-turn output;
renames appear as deletion/addition. Removal is never forced and keeps branches.
A provider's native model session requires a separate live verification.

Selected-file IPC fixtures run with `cargo test -p signaltty-server --test file_diff`.
They use real temporary repositories. The GUI file-diff display test uses the same
production row/reader controls with delayed actor replies to prove stale-response
handling. Under a headless environment, wrap its documented command with
`xvfb-run -a`; export `SIGNALTTY_UI_EVIDENCE` to inspect light, dark and narrow
reader captures. Binary, unavailable and truncated previews need explicit notices.
The reader test also copies selected text through the actual isolated clipboard,
returns focus to the selected row, loads the production stylesheet, checks semantic
colors in both themes, and verifies closed reader widgets are released after their
pending replies finish. The closed dialog releases its child from the native host.

The completed-history board test compares 25 versus 20 finished tasks, checks
the newest 20 rows, total heading and conditional subset notice, and exports
light/dark enlarged-text scenes through `SIGNALTTY_UI_EVIDENCE`.

Live board tests exercise production App refresh, dialog/row identity, metadata,
viewport anchors, narrow focused migration, rapid event bursts, newer focus,
original-column neighbor fallback, current pane activation, empty transitions
and close with pending restoration. Native GTK animations are allowed to finish
before asserting final geometry; no provider requests are made.
