# Local workflows

## Sub-features

Command/workspace palette, workspace rename, literal terminal search, pane zoom,
worktree create/open/remove and working-tree change summaries.

## How to get to it (user POV)

Use Ctrl+Shift+P for the palette, Ctrl+Shift+R to rename, Ctrl+Shift+F to search
and Ctrl+Shift+Z to zoom. The window menu also exposes these commands, Worktrees
and Show Changes. Worktree creation opens a shell workspace. Closing that
workspace preserves its checkout; removal is a separate confirmation.

## Driving it with GTK and real Git

Run `cargo build --workspace` then `cargo test -p signaltty-server --test worktrees`.
The suite drives actual IPC and the CLI executable against isolated repositories:
checkout isolation/reuse, literal paths, restore, branch retention, dirty/main/locked
refusal, background-process cwd checks and removal racing ordinary creation/launch.

Run each display test in its own process with `GTK_A11Y=none dbus-run-session --
cargo test -p signaltty-gui <name> -- --exact --ignored --test-threads=1`:

- `app::tests::navigation_palette_fast_enter_and_git_dialogs_use_native_controls`
- `app::tests::pane_zoom_keeps_hidden_terminals_and_restores_latest_ratios`
- `terminal::tests::terminal_search_is_literal_and_keeps_the_terminal`

Set `SIGNALTTY_UI_EVIDENCE=/absolute/evidence/path` for the navigation
test to export actual GTK window renders of palette/worktrees/changes in light
and dark themes. Terminal search and zoom tests export their scenes to the same
path when supported. Inspect every saved image before accepting it.

Observe immediate typing+Enter navigation, blank rename refusal, stable handles,
unchanged VTE objects, continued hidden output, unchanged server ratios, literal
file labels and binary/untracked indicators. Actor tests also prove slow worktree
requests keep the normal control and PTY stream available and are never retried.

## Gotchas

GTK dialog tests inject replies at the production IPC actor seam to exercise
widgets without mutating real checkouts. Real filesystem effects are established
separately by the server/CLI suite. The diff is against HEAD, not per-turn output;
renames appear as deletion/addition. Removal is never forced and keeps branches.
A provider's native model session requires a separate live verification.
