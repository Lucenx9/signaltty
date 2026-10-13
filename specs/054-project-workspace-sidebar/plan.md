# Implementation plan

Build native project selection without changing server session ownership. Follow
spec 054, the constitution and docs 02/06/08/09/14/17. Spec, architecture comparison,
plan and tasks precede code. Red tests cover core identity, real Git metadata and
the missing native selector. No constitutional deviation.

## Source grounding and decision

App derives summaries from WorkspaceCache and retains all snapshots for attention
and terminals. Keyed sidebar rows already support task hierarchy. Explicit user
navigation and background repaint are distinct paths; async creation/focus owns a
navigation ticket. New Workspace currently defaults to the active cwd.

Selected design A: additive GitInfo.primary_root, pure core catalog and client
scope. Design B joined worktree.list asynchronously in the GUI; it introduced
RPC/cache races and incorrectly pruned hidden rows. Independent Gemini judgment
and parent scoring agreed, A 23/25 versus B 13/25. Retain B's searchable native menu,
path disambiguation and scoped folder default. Correct the child visibility rule:
an included child whose parent is excluded becomes a root. ADR-0033 records the
metadata boundary. Initial Claude/Grok attempts hit provider limits; available
Codex/Gemini alternatives completed the comparison without permission prompts.

## Model and server

ProjectId is a typed directory key, stable when a folder becomes a Git repository.
ProjectScope is All or one ID. ProjectCatalog::derive groups primary_root, legacy
checkout root or cwd, sorts deterministically and retains workspace membership.
Reconciliation preserves valid scopes, migrates unambiguous legacy keys and only
resets missing scopes after a successful complete workspace list.

The server parses the first NUL-delimited Git worktree record. GitInfo.root retains
its existing checkout meaning. Creation and refresh enrich primary_root; a finite
async startup queue backfills legacy snapshots with a two-second probe deadline.
A guarded Store transition pairs mutation and workspace.updated; successful changes
mark persistence. Git does not run in core or GTK. No new IPC method.

## GUI

ProjectPicker owns a GtkMenuButton/Popover with SearchEntry, ListBox choices, paths,
counts, selection checks and an explicit no-match state. Native activation, arrows,
Escape and focus restoration remain usable. The control is pinned below Search.
Paths and long names ellipsize with full-path tooltips; popup width respects the
window. Original symbolic icons, Signal colors and compact controls remain.

Sidebar retains every keyed row. Its scoped projection sorts visible groups,
demotes excluded parents and combines project visibility with task collapse.
Reconcile suppresses selection echoes. Scope switches do not replace terminals.
Global palette and attention use the full catalog and reveal explicit targets;
background repaint preserves scope. New Workspace uses scoped folder before active
cwd, leaving editable validation, retry and pending-close behavior unchanged.

App configures separate project-scope.json storage before its first production
refresh. Existing GTK fixtures default to All without persistent storage; the new
fixture injects an owned temporary path. Atomic writes preserve appearance settings.

## Delivery proof

Core tests cover grouping, aliases, duplicate names and authority. Real Git tests
cover main/subdirectory/linked membership, literal paths and legacy restart.
Native tests cover filtering, row/terminal identity, collapse, search/no-match,
keyboard dismissal, attention navigation, creation defaults and persistence.
Inspect light/dark, 360px, enlarged, high-contrast and RTL renders. Run the shared
full gate on a clean revision, then independent Standards and Gemini Spec/UX reviews.
Retain selected evidence and open/link the PR against main.
