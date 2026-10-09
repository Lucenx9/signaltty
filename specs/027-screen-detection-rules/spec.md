# Specification: Screen-detection rules in agent manifests

Created: 2026-10-09. Status: clarified.

Agents without hooks today report only title, BEL, OSC and exit, so a hook-less
pane never shows `working` or `blocked`. herdr classifies these panes from the
screen with declarative per-agent rules (`src/detect/manifests/*.toml`). This
spec adds the same layer as data in the existing detection overlays, fulfilling
docs/14 directive 3 ("declarative per-agent screen-detection manifests where
hooks don't"). It stays ADR-0006's last-resort layer: hooks always dominate.

## Acceptance

1. A manifest may declare `[[screen]]` rules: `id`, `state`
   (`working`/`blocked`/`idle`), `region` (`title`, or `bottom` with `lines`,
   default 12, max 200), one or more `regex`, and `priority` (default 0).
   An invalid state, region, line count or regex rejects the whole manifest at
   load, logged and skipped like today's malformed manifests.
2. Live panes whose agent kind has rules are evaluated about twice a second
   against the visible screen and the pane title. The highest-priority matching
   rule wins, ties going to file then declaration order. No match changes nothing:
   the screen never invents a state.
3. `working` sets lifecycle `working`. `blocked` sets `blocked` and raises
   `input_required`. `idle` after `working` sets `done` and raises `unread`;
   `idle` after `blocked` sets `idle`; otherwise `idle` changes nothing.
   Leaving `blocked` withdraws its `input_required` (never other attention), so
   an answered prompt can still end its turn `unread`. Transitions emit the
   existing lifecycle/attention events and persist.
4. A pane that has received any `hook-event` for its current process is never
   classified from the screen (hooks dominate). That first hook withdraws an
   `input_required` a screen rule raised, never other attention. A new process
   (spawn, resume) starts unhooked again.
5. Rules from every manifest of the same kind apply, so several
   `kind = "generic"` manifests can each describe a different unknown agent.
6. Real-PTY integration tests prove blocked and working→done classification,
   hook dominance, and that a pane without rules is untouched.

## Scope and clarification

No bundled rules ship in this change; porting herdr's per-agent rules
(Apache-2.0) is the next slice, with its own fixtures. No new IPC method:
explaining why a pane holds a state (`agent.explain`) is deferred. Evaluation
uses the server's headless vt100 screen, never the GUI. Rule selection is by
the pane's agent kind only; matching rules to a specific binary is deferred.
No unresolved requirements remain.
