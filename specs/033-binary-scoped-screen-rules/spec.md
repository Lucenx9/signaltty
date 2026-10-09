# Specification: Screen rules scoped to a program, for more agents

Created: 2026-10-09. Status: clarified.

herdr recognises 22 agents; signaltty bundles screen rules for the five with
their own `AgentKind`. Every other agent runs as a `generic` pane, and a
generic manifest's `[[screen]]` rules apply to every generic pane, shells
included, so herdr's rules for Gemini, Copilot and the rest cannot ship. This
scopes generic rules to the program a pane is running and ships a first batch
of herdr's rules, per docs/14 directive 3.

## Acceptance

1. The server tracks each live pane's process name: the spawn argv's program
   at start, then the deepest non-shell descendant on every process refresh.
   For a `node` (`nodejs`), `bun`, `deno` or `python` runtime it is the script it runs.
   It is not persisted and emits no event.
2. A `generic` manifest that lists `binaries` applies its `[[screen]]` rules
   only to generic panes whose process name is one of them. Without
   `binaries` it still applies to every generic pane. Other kinds unchanged.
3. User rules replace bundled rules per scope: a generic pane uses the user
   generic rules that apply to it if any, else the bundled ones that do.
4. signaltty bundles herdr's rules for `gemini`, `copilot`, `droid`, `kilo`
   and `qodercli` (Apache-2.0, attributed), as generic manifests with herdr's
   binary names, each ending in the idle fallback.
5. `pane.explain` reports the `process` name; fixtures cover each new rule;
   real-PTY tests prove scoping (same output, different program) and the
   bundled rules for a renamed program.

## Scope and clarification

Agents whose herdr rules read OSC progress (amp, grok, kiro, letta, qwen) or
need extra regions come in a later slice. Shell scripts are named by the
shell (herdr unwraps them; not needed for these five). Fixtures are written
from the rules' text. No unresolved requirements remain.
