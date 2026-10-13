# Local workflows

## Sub-features

Command/workspace palette, workspace rename, literal terminal search, pane zoom,
worktree create/open/remove, working-tree change summaries and per-file diff review.

Project scope: open Choose Project below Search, search by name/path and choose
one project or All projects. Scope filters the workspace list while global attention
and the palette still reach all workspaces. New Workspace proposes the scoped folder.

`app::tests::project_scope_filters_without_rebuilding_workspaces` checks the native
selector, search/no-match/activation/focus, persisted scope, global attention count,
explicit navigation, creation default, passive refresh and row/terminal identity.
It emits light/dark, duplicate/long-name, 360px, enlarged-text and RTL renders.
`sidebar::tests::project_projection_retains_collapsed_groups_and_shows_external_parent_children`
checks cross-project task children and retained collapse. Core project tests cover
authority and legacy migration; real Git worktree tests cover canonical membership
and persisted metadata backfill after restart. Native high contrast is also run
with `ADW_DEBUG_HIGH_CONTRAST=1`; CSS-class scenes alone do not certify native HIG.

## How to get to it (user POV)

Use Ctrl+Shift+P for the palette, Ctrl+Shift+R to rename, Ctrl+Shift+F to search
and Ctrl+Shift+Z to zoom. The window menu also exposes these commands, Worktrees
and Show Changes. Worktree creation opens a shell workspace. Closing that
workspace preserves its checkout; removal is a separate confirmation.
Show Changes (Ctrl+Shift+D) docks a panel beside the terminals and follows the
active workspace. Activate a file to read its numbered patch. Back returns to the
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
- `app::tests::task_board_fits_narrow_windows_and_reveals_last_column`
  checks two-line titles and visible Previous/Next navigation through overflow,
  including control bounds with enlarged high-contrast text.
- `app::tests::preferences_preview_schemes_and_themes`
  checks paired theme miniatures, selection persistence and wrapping at 360px.
- `app::tests::shell_empty_states_explain_actions_and_fit_large_text`
- `app::tests::workspace_header_context_survives_shell_agent_and_empty_transitions`
- `app::tests::navigation_palette_fast_enter_and_git_dialogs_use_native_controls`
- `app::tests::palette_keyboard_selection_stays_visible_while_search_keeps_focus`
- `app::tests::palette_empty_results_and_shortcuts_fit_narrow_appearances`
- `app::tests::file_diff_reader_uses_native_numbered_selectable_controls`
- `app::tests::pane_zoom_keeps_hidden_terminals_and_restores_latest_ratios`
- `terminal::tests::terminal_search_is_literal_and_keeps_the_terminal`

Set `SIGNALTTY_UI_EVIDENCE=/absolute/evidence/path` for the navigation
test to export actual GTK window renders of palette/worktrees/changes in light
and dark themes. This fixture loads production CSS and checks branch/path
hierarchy, compact checkout content and filtered palette group/target mapping.
Terminal search and zoom tests export their scenes to the same
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

## Automatic workspace names

New Workspace defaults to a first-message AI title for Codex/Claude. Switch off
Automatic name for the native editable name field. The folder name is provisional;
manual Rename Workspace takes precedence, including during generation.

`cargo test -p signaltty-server --test workspace_titles` uses fake metadata CLIs
through a real isolated server: opt-in, first prompt, immediate hook response,
stable handles, single attempt, manual rename races, failures and persistence.
Server unit tests cover invalid output, Claude tool disabling, naming events and
interrupted restore. The shell empty-state display test also exports
workspace-panel and workspace-create desktop/large light/dark scenes, checks
status is below the title, and exercises automatic/manual name validation.
These tests do not contact paid providers or certify account/model compatibility.

## Reliable creation and compact tools

`app::tests::workspace_creation_retains_draft_and_retries_only_the_failed_command`
drives production form actions against deterministic actor failures: invalid
manual name/custom command cannot submit, pending draft cannot dismiss or submit
twice, workspace-create failure retains editable choices, startup failure preserves
the created id and freezes its folder/name, corrected command retries without a
duplicate workspace, and success focuses the VTE. Exports light/dark validation,
pending and partial-failure renders plus enlarged narrow/high-contrast validation.

`new_workspace::tests::dialog_validation_explains_invalid_folder` checks a missing
folder's visible guidance. `dialog_confirm_emits_validated_request` checks retained
draft, pending state and explicit completion/retry at the form seam.

`app::tests::visible_workspace_tools_open_palette_and_board_without_main_menu`
checks wide header entry points, labeled compact sidebar controls, actual Board
activation and Changes toggle synchronization. Exports 360px light/dark and
large-text/high-contrast compact controls. These are isolated GTK scenes, not proof
of screen-reader/OSK/IME or a real folder chooser.

Icon/placement acceptance: `owned_icon_roles_resolve_and_render_without_external_themes`
resolves bundled symbolic roles with external search paths disabled and exports
native 16/32px light/dark sheets. The toolbar test also checks New Tab compact
bounds, RTL group order and keyboard focus. Worktree navigation captures narrow
enlarged text and asserts Create/Refresh targets stay inside the window. The
Details fixture checks decorative roles and still proves Copy Path and Changes
handoff. Run the icon, toolbar and Details fixtures with
`ADW_DEBUG_HIGH_CONTRAST=1` for native high-contrast sheets/scenes.
