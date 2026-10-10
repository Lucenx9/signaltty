# Specification: `pane.rename` (pane names)

Created: 2026-10-10. Status: clarified.

herdr agents have unique names (`agent.rename`), and scripts target an agent
by its name instead of an opaque id. In signaltty every call needs the
generated `pane_…` id.

## Acceptance

1. `pane.rename {pane_id, name?}` sets the pane's `name`. A missing or `null`
   name clears it. Returns `{pane}`, emits `pane.updated` and persists.
2. A name is a lowercase slug, `[a-z][a-z0-9_-]*`, at most 32 bytes (herdr's
   rule), and is unique across the server. Otherwise → `BAD_PARAMS`. An
   unknown pane → `NO_SUCH_PANE`.
3. In every request, `pane_id`, `target_pane_id` and `parent_pane_id` accept
   a name in place of the id. A real id wins.
4. Documented in `docs/08`, `method::ALL`, the CLI (`pane rename`), and an
   integration test (rename, address by name, restart, clear, errors).

## Scope and clarification

The resolution happens once, in the router, before dispatch. This covers
every pane-addressed method, current and future, without touching each
handler.

Any pane can be named, not only agents. herdr limits names to agents
because only agents are targets of its `agent.*` API, and signaltty has no
separate agent namespace.

Naming at spawn time (`pane.spawn`/`task.start` `name`) and showing the name
in the GUI are left for later. `label` stays the free-text display label.
