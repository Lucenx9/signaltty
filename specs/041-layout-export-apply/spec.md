# Specification: `layout.export` / `layout.apply`

Created: 2026-10-10. Status: clarified.

herdr can export a tab's layout as a portable tree and start a new tab from
one (`layout.export` / `layout.apply`), so a grid of agents can be saved and
recreated. signaltty only has `tab.set_layout`, which rearranges panes that
already exist.

## Acceptance

1. `layout.export {tab_id}` returns `{workspace_id, tab_id, title, root}`.
   `root` has the `tab.set_layout` shape, and each `pane` leaf adds the pane's
   `cwd`, `argv` and `name`. An empty tab gives `root: null`. An unknown tab
   → `NO_SUCH_TAB`.
2. `layout.apply {workspace_id, title?, root}` starts a new, active tab
   (default title `layout`) with one new pane per leaf:
   - the leaf's `argv`, defaulting to the user shell;
   - the leaf's `cwd`, defaulting to the workspace folder;
   - the leaf's `name`, if set.

   Split directions and ratios are kept, with ratios clamped. Leaf `pane_id`
   is ignored, so export output applies as is. The method returns
   `{tab, panes}` and emits `tab.created`, then `pane.created` for each pane.
3. All or nothing: every leaf is validated before anything starts. A bad
   `cwd`, an empty `argv`, an invalid or taken name, a name repeated in the
   tree, a non-finite ratio, or more than 32 panes → `BAD_PARAMS`. If a spawn
   fails, the panes already started are stopped → `SPAWN_FAILED`, and nothing
   is created.
4. Documented in `docs/08` and `method::ALL`, exposed in the CLI
   (`tab export`, `tab apply`), and covered by an integration test.

## Scope and clarification

- herdr's `tab_id` (replace a tab in place), `focus` and per-leaf `env` are
  left out. To replace a tab, apply the new layout, then `tab.close` the old
  one. Both are visible to clients.
- Names stay unique: applying an export while the named source panes live
  fails, so the caller drops or changes the names.
- New panes start at 80×24, and the GUI resizes them when it shows the tab.
