# ADR-0026: Prompt-aware screen regions and a `hold` state

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/030-claude-screen-rules/`.

## Context

ADR-0025 bundled herdr's rules for Pi, OpenCode and Cursor and deferred
Claude, whose rules read the prompt box, the line above it and the text after
the last horizontal rule, and use `unknown` + `skip_state_update` to keep
transcript viewers and pickers from being read as a turn state.

## Decision

1. **Regions are a slice plus an optional line limit.** `screen`,
   `prompt_box`, `above_prompt_box` and `after_last_rule` slice the raw screen
   lines with herdr's definitions (rule = leading `─`, three or more or
   nothing after; box = second-last rule to the next); the existing
   normalization (trim, drop empty lines, keep the last `lines`) then applies
   to every region. `bottom` stays `screen` with 12 lines, so ADR-0024 rules
   are unchanged.
2. **`hold` is a state that changes nothing.** It takes part in priority like
   any rule, so a high-priority hold masks lower working/blocked/idle rules,
   and `Store::apply_screen_state` ignores it.
3. **Claude is ported, minus `osc_progress`.** signaltty does not track OSC
   9;4 progress, so that rule is dropped; the title rules use the pane title.
   herdr's per-line `line_regex` keeps to one line by narrowing `\s` to
   `[ \t]`; order-free `contains` pairs inside `any` become `(?is)a.*b|b.*a`.

## Consequences

Claude sessions without signaltty hooks show working, blocked, done and held
states. Codex still needs herdr's prompt-marker regions and remains unported.
The flattening makes some rules longer than herdr's; fixtures pin each one.
