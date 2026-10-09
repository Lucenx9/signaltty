# Specification: Claude screen rules and prompt-aware regions

Created: 2026-10-09. Status: clarified.

Spec 028 deferred Claude's herdr rules because they read regions the engine
lacked: the prompt box, the line above it, and the text after the last
horizontal rule. A Claude session started without signaltty's hooks (typed in
a shell, no `integration install`) therefore shows no state. This adds those
regions, a `hold` state, and ships Claude's rules, per docs/14 directive 3.

## Acceptance

1. `region` accepts `screen`, `prompt_box`, `above_prompt_box` and
   `after_last_rule` besides `title` and `bottom`, following herdr's
   definitions: a horizontal rule is a line starting with `─` (at least three,
   or nothing after them); the prompt box is the text between the second-last
   rule and the next rule. `lines` (1..=200) keeps the last non-empty lines of
   any non-title region; `bottom` stays `screen` + `lines = 12`. A missing
   box or rule falls back as in herdr (empty box, whole screen otherwise).
2. A rule may have `state = "hold"`: when it wins, the pane's state is left
   unchanged (herdr's `skip_state_update`), so a transcript viewer or model
   picker cannot be read as working, blocked or idle.
3. signaltty ships Claude rules ported from herdr (Apache-2.0, attributed),
   ending in the idle fallback. herdr's `osc_progress` rule is omitted:
   signaltty does not track OSC 9;4 progress.
4. Fixtures cover each Claude rule (working spinner and status line,
   background agents above the box, permission prompts, forms, MCP input,
   transcript and model picker holds, title states, idle prompt box) and
   priority conflicts between them. `pane.explain` labels the new regions.

## Scope and clarification

Codex rules need herdr's prompt-marker regions and are the next slice.
Fixtures are written from the rules' text, not captured from Claude Code.
Rules only apply until the pane's first hook, unchanged from spec 027.
No unresolved requirements remain.
