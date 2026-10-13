# Code reference and decisions

Inspected the user's readonly T3 checkout `/home/simone/code/t3code` at 740bda40c9, with primary origin Lucenx9/t3code and upstream pingdotgg/t3code. The earlier reference under `/tmp/t3ref/t3code` is 50647de0. No reference checkout was changed.

- [Sidebar source](https://github.com/Lucenx9/t3code/blob/740bda40c9/apps/web/src/components/Sidebar.tsx): project scope near line 2532, persisted UI state, All projects default, searched project collection, filtered session membership, stale-scope reset after all live project snapshots. The menu around line 4841 uses a native-to-web Combobox and restores full scope through the default entry.
- [Project grouping](https://github.com/Lucenx9/t3code/blob/740bda40c9/apps/web/src/sidebarProjectGrouping.ts): logical project groups feed that selector. Signaltty has one server and no remote environments; location identity is sufficient here.
- [Sidebar header](https://github.com/Lucenx9/t3code/blob/740bda40c9/apps/web/src/components/sidebar/SidebarThreadHeader.tsx): fixed header keeps search and project context above the scrolling sessions.

Translation: native GtkMenuButton/Popover, searched GtkListBox choices, pinned scope label, existing workspace rows and priority/task tree. Persist client scope independently of appearance. Reuse original folder/check/search icons and actual App actions. Do not copy React/BaseUI implementations or asset files.

Signaltty source trace: `GitInfo.root` identifies each checkout; server git_info fills it only on create/refresh. Main checkout and linked worktrees therefore need additive primary-root metadata. GUI WorkspaceCache keeps last good snapshots on failure, and only authoritative workspace.list removes entries. App show_workspace_internal already distinguishes user navigation from event refresh; this must also govern revealing scoped-out workspaces. Global attention derives all cached panes and must remain global.

Project registration CRUD was considered but exceeds the requested sidebar selection. Pure cwd-prefix grouping was rejected because it merges unrelated nested repos and separates linked worktrees. A name-based grouping was rejected because identical basenames collide. Domain/UI choice was finalized by the architecture sketch before source edits.
