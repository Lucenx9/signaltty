# ADR-0019: GUI themes via per-window libadwaita variable overrides

**Status**: Accepted, 2026-10-04. **Spec**: [017-themes](../../specs/017-themes/spec.md).

## Context

`docs/06` (principle II) says colour comes only from libadwaita variables so
the system accent follows the desktop. The product direction (`docs/14` §7)
asks for named themes with their own accents, modelled on t3code appearance
settings. A theme must recolour chrome and accent per window, keep every
status meaning readable against every accent, keep the terminal legible
without retinting the shell's ANSI palette, and keep high contrast winning
over theme subtlety.

## Decision

- The window root carries exactly one theme class — `theme-signal`,
  `theme-grove`, `theme-ocean`, `theme-ember` or `theme-iris` — alongside
  the existing `dark`, `high-contrast` and `reduced-motion` classes.
  Light selectors (`.theme-<id>`) redefine libadwaita's own variables
  (`--window-/-view-/-headerbar-/-sidebar-/-card-/-popover-/-dialog-bg-color`
  and friends); dark selectors (`.theme-<id>.dark`) override the `.dark`
  block. All hexes are from `specs/017-themes/research.md` §2.
- Signal keeps the desktop accent: Signal light sets nothing (stock
  Adwaita) and Signal dark is the shipped `.dark` values byte-for-byte.
  Every other theme replaces the desktop accent by explicit user opt-in
  in Preferences. This is the documented deviation from `docs/06`: the
  replacement happens only through an explicit choice, and the default
  still follows the desktop exactly as today.
- Standalone colours are set explicitly. `:root` computes
  `--accent-color` (and `--warning-`/`--error-color`) from the stock bg
  via `oklab()`, and overriding a bg on a descendant theme class does
  not recompute them — so each theme sets its accent text hex, and Ember
  sets its warning/error text hexes too.
- Ember alone retints status (error to hue 16, permission to 74, warning
  to 110, all ≥ ΔE 14 apart), because its coral accent shares the warm
  band with approval orange and input amber. No other theme touches
  status tokens. Permission styling points at two app tokens,
  `--permission-color` / `--permission-bg-color`, instead of the shared
  GNOME `--orange-*` ramp, so unrelated Adwaita widgets are unaffected.
- The terminal takes only the theme's pane colour: `.pane` paints
  `var(--pane-bg-color)` and the VTE background mirrors the same hex
  from a static Rust table (validated against the CSS by test). The
  16-colour ANSI palette and the VTE foregrounds are fixed.
- Workspace marks move only where they collide with an accent (Ember
  tint-1/tint-2, Iris tint-0/tint-2 become teal/steel spares), and every
  dark glyph gets lighter so it clears 4.5:1 on dark sidebars.
- The high-contrast variable overrides sit strictly after all theme
  blocks with per-theme selectors (`.theme-<id>.high-contrast`,
  `.theme-<id>.dark.high-contrast`), so a concrete theme alpha can never
  outrank them. The theme accent is kept; secondary text is restored by
  inheritance, not by a weaker fg token.
- Preferences swatches use literal hexes per theme × variant, never the
  variables, because each swatch carries its own `theme-<id>` class
  inside a window themed differently.

## Consequences

Signal renders exactly as before in both variants (same variables, same
resolved values) except dark workspace marks, which get lighter glyphs
per the contrast fix in research §4. Coloured themes are pure CSS on top
of the same widget rules: no widget rebuilds, no terminal restarts, no
scrollback loss on switch. The Rust side owns class toggling,
`adw::StyleManager` scheme sync, persistence in `gui.json`, and the pane
hex table; the CSS owns every colour seen on screen. Adding a sixth
theme means one light block, one dark block, a swatch pair, and a
collision check against status hues — no widget changes.
