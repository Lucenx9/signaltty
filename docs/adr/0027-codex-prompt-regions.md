# ADR-0027: Codex prompt-marker regions

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/031-codex-screen-rules/`.

## Context

ADR-0026 added the Claude prompt-box regions. Codex's herdr rules read the
screen relative to its `›` prompt line instead, and the first lines of the
screen for the trust prompt. Codex was the last agent of herdr's set without
bundled signaltty rules.

## Decision

1. **Three prompt-marker regions, with herdr's definitions.** A prompt line
   is `›` alone or starting with `› `; the current prompt is the last prompt
   line with no `•`, `■`, `✗` or `✓` block line after it.
   `after_last_prompt` reads after the last prompt line, `before_current_prompt`
   above the current prompt, and `without_current_prompt` the whole screen
   only when there is no current prompt. Without a prompt line both prompt
   regions read the whole screen, as in herdr.
2. **`top` keeps the first `lines`.** It is the screen with the line limit
   taken from the start (default 12), for herdr's `top_non_empty_lines`.
3. **Codex is ported whole.** Nested gates become split rules
   (`trust_folder`, `live_strong_picker`) or alternations; `unknown` +
   `skip_state_update` becomes `hold`.

## Consequences

All five herdr agents signaltty knows (Pi, OpenCode, Cursor, Claude, Codex)
now have bundled screen rules. The prompt-marker regions are Codex-shaped (the
`›` glyph and block markers are hardcoded, as in herdr); another agent with a
different prompt would need its own region. `top` normalizes lines like every
region, so herdr's `\A` anchor matches the first non-empty line, not a
leading empty one.
