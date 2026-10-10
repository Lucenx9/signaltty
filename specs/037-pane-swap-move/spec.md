# Specification: `pane.swap` and `pane.move`

Created: 2026-10-10. Status: clarified.

herdr scripts can swap two panes and move a pane next to another. In
signaltty the only way is `tab.set_layout`, which works within one tab and
cannot change the tab a pane belongs to.

## Acceptance

1. `pane.swap {pane_id, target_pane_id}` exchanges the two panes' slots. The
   tree shape and the ratios do not change. Across tabs, each pane changes
   tab and each tab's focus stays on its slot.
2. `pane.move {pane_id, target_pane_id, direction?}` removes the pane from its
   layout, collapsing the parent split, and splits the target with it
   (`right` by default), evening out the run like `pane.split`. A source tab
   left without panes is empty, as after `pane.close`.
3. Processes, PTYs and output are untouched. Each touched tab emits
   `tab.updated`. A pane that changed tab emits `pane.updated`. The result is
   `{tabs}`.
4. The same pane twice, panes in different workspaces, or a pane that is not
   in its tab layout → `BAD_PARAMS`. An unknown pane → `NO_SUCH_PANE`.
5. Both are in `docs/08`, `method::ALL`, the CLI (`pane swap`, `pane move`)
   and integration tests.

## Scope and clarification

Moves across workspaces are out of scope. A workspace owns its cwd and git
state, so a pane would carry a stale context. herdr's `pane.zoom` is not
added. Zoom in signaltty is a per-window GUI view that is cleared on
navigation, so a server flag would fight it.
