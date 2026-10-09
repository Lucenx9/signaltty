# ADR-0028: Scope generic screen rules to the pane's program

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/033-binary-scoped-screen-rules/`.

## Context

herdr recognises 22 agents; signaltty's `AgentKind` stays closed (ADR-0009),
so every other agent is a `generic` pane. A generic manifest's `[[screen]]`
rules applied to every generic pane, shells included, which made herdr's
per-agent rules unshippable for those agents.

## Decision

1. **Generic rules follow `binaries`.** A `generic` manifest with `binaries`
   applies its screen rules only to generic panes running one of them;
   without `binaries` it applies to every generic pane, as before. For a
   specific kind, `binaries` stays a detection alias list and rules apply to
   the whole kind.
2. **The Store tracks the process name, not the model.** It is set from the
   spawn argv when a process begins and refreshed by procscan from the
   deepest non-shell descendant (a `node`/`bun`/`deno`/`python` runtime is
   named after its script). It is per-process server state: not persisted,
   no event, so the procscan tick stays quiet.
3. **Replacement is per applicable scope.** A pane uses the user rules that
   apply to it if there are any, else the bundled ones that do.
4. **Bundled generic manifests use herdr's binary names**, starting with
   gemini, copilot, droid, kilo and qodercli.

## Consequences

New agents become a TOML file. A program launched from a shell is picked up
on the next 10 s refresh. Shell scripts are named after the shell; herdr's
deeper wrapper unwrapping is not needed for these agents. Agents whose herdr
rules read OSC progress need that signal first.
