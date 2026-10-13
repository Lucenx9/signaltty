# Feature specification: Selectable project sidebar

Created 2026-10-13. Branch `gui/project-workspace-sidebar`, base main 553343f, including merged icon fix PR #102.
Input: keep workspace icons, make the project selectable with its workspaces like T3 Code, inspect its code and publish a PR.

## User scenarios

### US1 Choose a project (P1)

A user opens the sidebar project selector, searches project names/paths, chooses one and sees its workspaces. All projects restores the full list. Names are recognizable; same-named repositories remain distinguishable by path.

Acceptance: native selector is pinned above the scrolling list; displays current scope. Project switch filters rows without restarting/rebuilding terminals or silently changing the active terminal. Search has an explicit no-match state; Escape dismisses and returns focus. A fresh empty application still has a usable All projects entry and New Workspace action.

### US2 Keep related work together (P1)

The main checkout, subdirectories and linked Git worktrees share the same project. Independent repositories never merge merely because their names match. A non-Git workspace belongs to its folder project.

Acceptance: existing workspace and task rows, child disclosure, priority order, names/metadata and icons remain intact. Project selection survives application restart and event refresh. A scope removed from a complete live catalog falls back to All projects; a failed/disconnected refresh does not discard it. Last-workspace closure removes the derived project. Selecting a workspace through global palette/attention reveals its project; ordinary event refresh does not reset the user's scope.

### US3 Start work in the selected project (P2)

New Workspace uses the selected project's folder as its default even if the active terminal belongs to another project. The form remains editable, retains its existing validation/retry and safe pending-close behavior. After creation the workspace is visible in the correct scope. Attention controls continue to count and reach workspaces outside the scope.

## Requirements

- FR001: Provide a selectable project scope with All projects and searchable project choices, using the T3 Code sidebar behavior as reference.
- FR002: Derive identity from repository/folder location, not display name; linked checkouts and nested repository paths must share the canonical primary project.
- FR003: Filter workspace rows in place, preserving task-child hierarchy, selection semantics, collapse state, keyboard actions, scroll and terminal identity.
- FR004: Retain global attention and navigation. User navigation into an excluded workspace reveals it; background refresh never overrides a valid scope.
- FR005: Persist scope as client presentation state independently of appearance settings and server session ownership. Recover stale persisted scopes only after a complete live catalog.
- FR006: Project-scoped creation must prefill its folder, keep the draft editable, and preserve all safety/retry behavior.
- FR007: Verify light/dark, 360px and narrow rail, enlarged text, native high contrast, long/duplicate names, keyboard focus and empty/disconnected states.
- FR008: Preserve old snapshots and clients through defaulted additive metadata; no GUI subprocess/Git access or core OS calls; server owns canonical repository enrichment.

## Edge cases

Only task children open after their parent closes; nested task lineage; main checkout not open but linked checkout present; linked worktrees upgraded from older snapshots; same folder names under different parents; spaces/non-ASCII/newline paths; missing/unavailable Git; legacy missing metadata; scoped project removed; partial snapshot loads; active workspace outside filter; project switch while pending creation; enlarged/mirrored layouts.

## Success criteria

Native project selection reveals exactly its existing workspaces without new terminals. Main/subdirectory/linked-checkout fixtures group correctly and duplicate names do not collide. Keyboard/project filtering/global attention and project-specific creation work through production App widgets/actions. Full shared verification, actual native captures and independent review pass.

## Assumptions and scope

The examined T3 Code implementation uses a persisted project-scope menu above its session list. Signaltty projects are a catalog derived from existing repository/folder workspaces, not a second session container or independently managed empty-project registry. New Workspace remains the creation flow. Retain current status vocabulary, original icon family and native GTK/libadwaita interactions. Only presentation and necessary additive Git metadata are authorized; no remote-project management or server Project CRUD.
