# 16 — Visual References: Geist, Linear, Vercel (beyond t3code)

Why look past t3code: it sets the *rendering language* (directive 6) but
not the *component discipline*. Vercel's Geist is a published token system
(values included); Linear is the reference for the attention loop's
interaction patterns (inbox, keyboard-first). Both constrain what
"Linear-grade" means for `docs/14` §7 without breaking the GNOME-citizen
rule: **translate patterns, never paste hex** — Adwaita variables stay the
source of truth so light/dark/accent/high-contrast keep following the desktop.

## Sources (primary)

- Geist foundations + components: `https://vercel.com/geist/{colors,theme,
  button,badge,keyboard-input,command-menu,empty-state,…}` and the shipped
  CSS custom properties (light + dark pairs) fetched 2026-09-29.
- Linear product docs: `https://linear.app/docs/{inbox,notifications}.md`
  (llms.txt index), `https://linear.app/llms.txt`.
- Typeface evidence: `InterVariable.woff2` preloaded on linear.app;
  `--font-geist-sans` / `--font-geist-mono` stacks in Vercel CSS.

## Geist tokens (exact values, light / dark)

Neutral ramp is pure gray, steps shared by both themes:

| step | light | dark | used for |
|---|---|---|---|
| background-100 | `#fff` | `#000` | canvas |
| background-200 | `#fafafa` | `#000` | sunken |
| gray-100 | `#f2f2f2` | `#1a1a1a` | hover (light) / wash |
| gray-200 | `#ebebeb` | `#1f1f1f` | hover (dark) |
| gray-400 | `#eaeaea` | `#2e2e2e` | borders |
| gray-700 | `#a8a8a8` | `#8f8f8f` | secondary text / disabled fg |
| gray-1000 | `#171717` | `#ededed` | primary text |

Accents are per-theme pairs (700 step): blue `#0071f6` / `#0070f7`, red
`#fc0035` / `#f13242`, amber `#ffb200`, green `#28a948` / `#00ab3e`,
purple `#9f00f4` / `#9440d5`, teal `#00a794` / `#00a694`.

Elevation is layered, never a lone drop: `--ds-shadow-border-small` =
`0 0 0 1px border-base` + `0px 2px 2px …0a` + `0px 8px 8px -8px …0a` +
background-border; dark hairlines go *inset* (`#ffffff1a`) so splits can't
clip them — the same reason our pane rings are inset shadows.

Focus is a triple ring, instant (transition explicitly off on focus):
`0 0 0 1px border, 0 0 0 2px background, 0 0 0 4px focus-color`.

Components: buttons 32px tall, 6px radius, 14px medium, transitions 150ms
`ease-in-out` over border/background/color/transform/box-shadow only;
press = scale (no layout shift); kbd chips 20px tall, `text-xs`, rounded;
headings tracked tight (`letter-spacing: -0.32px` at 16px); disabled =
gray-700 on gray-100; empty states = icon + title + description + one
action (exactly our `AdwStatusPage` shape).

## Linear patterns (interaction, not hex — styles are CSS-in-JS)

- **Priority vs Other inbox tabs**: the triage split our "Needs you"
  section already mirrors — validates the design, keep it.
- **Keyboard-first lists**: `J`/`K` move, `U` toggles read, `H` snoozes,
  `Backspace` clears, `G I` jumps to inbox, `Cmd/Ctrl F` filters the list
  in place. Our sidebar has *none* of this: selection is pointer-only and
  `Ctrl+Shift+J` jumps globally without list context.
- **No browser-push novelty**: Linear routes real-time alerts to the
  desktop app and digests the rest by urgency — our `SIGNALTTY_NOTIFY`
  kill-switch + 8s timeout + suppress-when-focused already follows this;
  Focus/Mark-read actions (§007) match their "act from the alert" loop.
- **Inter everywhere**, tabular numerals in counts, 13px dense UI with
  6–8px radii and 1px dividers over a near-black canvas.

## Gap analysis vs `gui/data/style.css`

What already matches (do not touch): inset hairline rings, 150–200ms
single-curve motion, press-scale pills, status-vocabulary colors,
`AdwStatusPage` empty states, hover-reveal row controls, `.numeric` counts.

| # | Delta | Cost | Note |
|---|---|---|---|
| 1 | **Visible keyboard focus** (Geist triple ring, instant) on decision-bar buttons, sidebar rows, header button | CSS-only | Required by spec 003 ("keyboard reachable") yet unfunded: today focus is invisible on buttons. Highest value, no model change. |
| 2 | **Sidebar list keyboard nav** (`J`/`K` move, `U` mark read on the selected row, type-to-filter?) | GUI-only | Linear parity for the attention loop; reuses `pane.mark_seen` + selection. Needs a display to verify. |
| 3 | **Command palette** (`Ctrl+K`: jump to workspace/pane, run `win.*` actions) | GUI-only, medium | Subsumes `Ctrl+Shift+J`; Geist `command-menu` + Linear `⌘K` agree on the shape. Natural home for future `workspace diff` display too. |
| 4 | **Permission-orange audit**: `orange-3` hardcoded with one dark override vs Geist per-theme accent pairs | CSS-only | Either tune the pair or fold approval into the warning scale; eyeball light + dark. |
| 5 | **kbd chip style** for shortcut hints in menus/tooltips | CSS-only, tiny | Geist `kbd` pattern; cheap consistency win. |
| 6 | Snooze (`H`) | Model + IPC + GUI | Deferred: needs a `snoozed_until` attention dimmer server-side (new state, new spec). |
| 7 | Elevation layering (border + drop + bg-border) | CSS-only, subtle | Our single `0 1px 3px` is flatter than both references; low priority. |

Out of scope on purpose: Inter/Geist typefaces (system stack is the
GNOME-citizen rule), pasted hex values (Adwaita owns color), per-client
seen-states and turn-scoped diffs (ADR-0011 stands).

## Recommendation

Spec `008-command-palette-focus` covering deltas 1–3 (+ 4–5 as polish
inside it): keyboard focus rings first (unblocks the 003 promise),
sidebar `J`/`K`/`U` second, palette third. Implementation waits for a
display (screenshots light + dark are the gate per the constitution) —
this file is the research input, not the green light.
