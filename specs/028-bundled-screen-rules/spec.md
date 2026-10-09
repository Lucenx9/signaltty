# Specification: Bundled screen rules for Pi, OpenCode and Cursor

Created: 2026-10-09. Status: clarified.

Spec 027 added `[[screen]]` rules but shipped none, so out of the box a pane
running Pi (which has no hooks) still never shows `working`, `blocked` or
`done`. This spec ships herdr's screen rules for the agents whose rules fit the
engine with two small additions, following docs/14 directive 3.

## Acceptance

1. A `[[screen]]` rule may add `all` (every regex must match) and `not` (no
   regex may match) to `regex` (any regex matches). It needs `regex` or `all`;
   an invalid regex in any list rejects the manifest.
2. signaltty ships rules for `pi`, `opencode` and `cursor`, ported from herdr
   (Apache-2.0, attributed in each file), each ending with a lowest-priority
   `idle` rule: like herdr, a known agent that shows no working or blocked
   sign is idle, so a finished turn becomes `done` + `unread`.
3. If any user manifest of a kind declares `[[screen]]` rules, those replace
   the bundled rules of that kind; otherwise the bundled rules apply. Hook
   gating and transitions are unchanged from spec 027.
4. Fixture tests drive each bundled rule set through a representative working,
   blocked (OpenCode, Cursor) and idle screen, plus near-miss screens.
   A real-PTY test shows a `pi` pane going working → done with no manifests.

## Scope and clarification

Claude and Codex rules need herdr's prompt-aware regions (`prompt_box_body`,
`after_last_prompt_marker`, …) and are deferred; those agents usually report
through hooks anyway. Fixtures are written from the rules' text, not captured
from the real CLIs, so they prove the port, not the CLIs' current output.
`agent.explain` stays deferred. No unresolved requirements remain.
