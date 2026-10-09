# Specification: Codex screen rules and prompt-marker regions

Created: 2026-10-09. Status: clarified.

Codex is the last agent of herdr's set whose rules signaltty lacks. They read
regions anchored on Codex's `›` prompt line and the first lines of the
screen. A Codex session started without signaltty hooks therefore shows no
state. This adds those regions and ships Codex's rules, per docs/14
directive 3.

## Acceptance

1. `region` accepts `top` (the first `lines` non-empty lines, default 12),
   `after_last_prompt`, `before_current_prompt` and `without_current_prompt`,
   following herdr: a prompt line is `›` alone or starting with `› `; the
   current prompt is the last prompt line with no `•`, `■`, `✗` or `✓` block
   line after it. Without a prompt, `after_last_prompt` and
   `before_current_prompt` read the whole screen; `without_current_prompt` is
   the whole screen only when there is no current prompt, otherwise empty.
2. signaltty ships Codex rules ported from herdr (Apache-2.0, attributed),
   ending in the idle fallback.
3. Fixtures cover each Codex rule (title blocked/working/idle, transcript
   hold, trust directory and folder prompts, startup update, strong and weak
   blockers, the timed working status and its reconnect-failed veto) and the
   region edge cases; `pane.explain` labels the new regions.

## Scope and clarification

Fixtures are written from the rules' text, not captured from Codex. Rules
only apply until the pane's first hook, unchanged from spec 027. No unresolved
requirements remain.
