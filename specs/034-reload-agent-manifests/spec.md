# Specification: Reload agent manifests without restarting the server

Created: 2026-10-09. Status: clarified.

Agent manifests (`agents/*.toml`: detection overlays, hook overrides, screen
rules) load once at server start. Editing a screen rule or adding a program
needs a server restart, which drops every live pane's process. herdr reloads
manifests on request (`server.reload_agent_manifests`); plugins already do
here (`plugin.reload`). This adds the same for agents, per docs/14
directives 3 and 4.

## Acceptance

1. `agents.reload` re-reads the agents directory and atomically replaces the
   loaded manifests; `agents.list` returns the current ones. Both answer
   `{dir, manifests: [{file, kind, binaries, screen_rules}], failures: [{file,
   error}]}`; a malformed file is listed in `failures` and skipped, never fatal.
2. After a reload, spawn/hook detection, hook overrides, screen rules and
   `pane.explain` use the new manifests. The reload itself does not touch
   live panes (they keep their process and kind); the periodic process
   refresh may later promote a generic pane, as it does for any program
   started in a shell. A request in flight uses one consistent set. A missing
   agents dir is an empty set; a dir that cannot be listed fails the reload
   with `IO_ERROR` and keeps the active set.
3. `signaltty agents list` and `signaltty agents reload` print the same data
   (`--json` raw).
4. Real-IPC tests prove a reload adds, changes and removes screen rules for a
   live pane, and that a malformed file is reported without dropping the
   others. docs/08 gets the rows.

## Scope and clarification

No file watching: reload is explicit, like `plugin.reload`. Bundled rules
are compiled in and are not reloaded. No unresolved requirements remain.
