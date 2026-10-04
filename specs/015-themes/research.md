# Theme colour research

Five built-in themes, each with a light and a dark variant: Signal (default), Grove, Ocean, Ember, Iris. A theme recolours chrome and accent. The terminal takes only the theme's pane colour. The 16-colour ANSI palette in `terminal.rs` does not change.

Numbers below are WCAG 2.x relative luminance: sRGB linearized per the spec, contrast `(Llighter + 0.05) / (Ldarker + 0.05)`. Alpha is composited in 8-bit sRGB before measuring. Light body text is `rgb(0 0 6 / 80%)` composited on the surface. Dark body text is `#ffffff`. Hue is OKLCH (0 red, 90 yellow, 180 green, 270 blue). ΔE is CIEDE2000. Variable names were checked against the libadwaita 1.7.6 stylesheets in the GResource of `libadwaita-1.so.0` (`base.css`, `base-hc.css`, `defaults-light.css`, `defaults-dark.css`).

## 1. Token set

A coloured theme sets the rows marked "all". Signal dark sets only the rows marked "Signal dark", at the hexes already in `.dark` in `style.css`. Signal light sets nothing: accent follows the desktop, surfaces stay stock Adwaita.

| Token | Who sets it | Notes |
| --- | --- | --- |
| `--window-bg-color` | all, Signal dark | Canvas. |
| `--window-fg-color` | coloured themes | Dark `#ffffff`. Light `rgb(0 0 6 / 80%)`. Signal inherits the same. |
| `--view-bg-color` | all, Signal dark | |
| `--view-fg-color` | coloured themes | Same fg as window. |
| `--headerbar-bg-color` | all, Signal dark | Equal to that variant's window. |
| `--headerbar-fg-color` | coloured themes | Same fg as window. |
| `--headerbar-backdrop-color` | all, Signal dark | Equal to window. |
| `--headerbar-shade-color` | all, Signal dark | `transparent`. Stock dark would otherwise draw its shade line back. High contrast paints over this. |
| `--sidebar-bg-color` | all, Signal dark | |
| `--sidebar-fg-color` | coloured themes | Same fg as window. |
| `--sidebar-backdrop-color` | all, Signal dark | Dark: equal to sidebar. Light: its own step. |
| `--sidebar-border-color` | all, Signal dark | Neutral hairline, not hue-tinted. See shared tokens. |
| `--card-bg-color` | all, Signal dark | Dark: `alpha(#ffffff, 0.06)` on every theme, including Signal. Light coloured: that theme's view hex. |
| `--popover-bg-color` | all, Signal dark | |
| `--popover-fg-color` | coloured themes | Same fg as window. |
| `--dialog-bg-color` | all, Signal dark | |
| `--dialog-fg-color` | coloured themes | Same fg as window. |
| `--border-color` | all, Signal dark | Concrete alpha, not a surface hex. See doubts. |
| `--accent-bg-color` | coloured themes | One hex for both variants. White text on it is ≥ 4.86:1. |
| `--accent-fg-color` | coloured themes | `#ffffff`. |
| `--accent-color` | coloured themes | Variant-specific text. Must be set explicitly. See doubts. |
| `--pane-bg-color` | coloured themes; app token | Not a libadwaita variable. `.pane` background and the VTE background. Signal's unset default is light `#ffffff`, dark `#111114`. |

Ember also sets the status tokens in §3. No other theme sets status tokens.

Deliberately unset: `--secondary-sidebar-*`, `--headerbar-border-color`, `--headerbar-darker-shade-color`, `--sidebar-shade-color`, `--card-fg-color`, `--thumbnail-*`, `--destructive-*`, and the GNOME `--orange-1`…`--orange-5` ramp. Card fg already matches window fg. `--orange-*` is shared with unrelated Adwaita widgets; permission uses the app tokens below instead of retinting it.

| App token | Default light | Default dark | Ember |
| --- | --- | --- | --- |
| `--permission-color` | `#c64600` (`--orange-5`) | `#ffa348` (`--orange-2`) | variant text in §3 |
| `--permission-bg-color` | `#ff7800` (`--orange-3`, the ring) | same ring | `#9b670b` |
| `--pane-bg-color` | `#ffffff` | `#111114` | theme pane hex |

There is no libadwaita `--permission-*` and no `--pane-bg-color`. `--permission-bg-color` is the ring and the pill wash (`alpha(..., 0.16)`), not a text colour. `#ff7800` as text on white is 2.65:1.

### Name doubts

- `--border-color` is not a hex. In `base.css` it is `color-mix(in srgb, currentColor var(--border-opacity), transparent)` with `--border-opacity: 15%`. High contrast (`base-hc.css`) raises that opacity to 50%, dim to 90%, disabled to 40%. The app already replaces `--border-color` with a concrete alpha, which bypasses `--border-opacity`. A theme that sets the concrete alpha must be followed by a high-contrast rule that sets it again.
- `--accent-color`, `--warning-color`, `--success-color`, `--error-color`, and `--destructive-color` are specified on `:root` as `oklab(from var(--*-bg-color) var(--standalone-color-oklab))`. Light standalone is `min(l, 0.5)`; dark is `max(l, 0.85)`. They are computed on `:root`. Overriding the bg on a descendant theme class does not recompute them. A theme that changes a bg must also set the matching standalone colour. The same trap applies to `--warning-color` and `--error-color` on Ember.
- Signal must not set accent variables. Default GNOME blue `--accent-bg-color #3584e4` with white is 3.77:1. That failure belongs to the desktop accent, not to a theme hex. `blue-4 #1c71d8` would clear 4.77:1 and is not adopted.
- Dark workspace marks at the current alphas fail 4.5:1 (measured in §4). The mark hexes below are the proposed fix, not the colours in `style.css` today.

## 2. Palettes

### Ladder

Shipped Signal dark, OKLab L: window `#08080a` 0.135, sidebar `#0c0c0e` 0.155, pane `#111114` 0.179, view `#141418` 0.193, dialog `#18181c` 0.211, popover `#202024` 0.245. The comment in `style.css` calls the sidebar the deepest plane. The hexes do not: the window is darker. Coloured dark themes copy the shipped order, then tint. Light themes invert in the stock Adwaita role order: sidebar 0.928, sidebar-backdrop 0.952, window = headerbar = headerbar-backdrop 0.972, pane 0.984, view 0.991, dialog 0.994, popover 0.998.

Dark chroma is 0.018 against the Signal L of each role. Light chroma is 0.015. Both are reduced into sRGB. Near-white view, dialog, and popover lose chroma on purpose. Hex rounding moves surface hue a few degrees off the accent hue; the tint is the chroma, not a second accent.

### Shared tokens

| Token | Dark, every theme | Light coloured themes | Signal light (unset, stock) |
| --- | --- | --- | --- |
| window / view / headerbar / sidebar / popover / dialog fg | `#ffffff` | `rgb(0 0 6 / 80%)` | `rgb(0 0 6 / 80%)` |
| `--border-color` | `alpha(#ffffff, 0.09)` | `alpha(rgb(0 0 6), 0.14)` | mix at `--border-opacity: 15%` |
| `--sidebar-border-color` | `alpha(#ffffff, 0.07)` | `alpha(rgb(0 0 6), 0.10)` | `rgb(0 0 6 / 7%)` |
| `--headerbar-shade-color` | `transparent` | `transparent` | `rgb(0 0 6 / 12%)` |
| `--card-bg-color` | `alpha(#ffffff, 0.06)` | the view hex | `#ffffff` |
| headerbar bg and backdrop | window hex | window hex | bg `#ffffff`, backdrop `#fafafb` |

Hairlines stay neutral so a green or orange theme does not paint its hue into every edge.

### Signal

Dark, copied exactly. Light, not set; the resolved stock values are listed so contrast can be checked.

| Token | Light (stock, unset) | Dark (authored) |
| --- | --- | --- |
| `--sidebar-bg-color` | `#ebebed` | `#0c0c0e` |
| `--sidebar-backdrop-color` | `#f2f2f4` | `#0c0c0e` |
| `--window-bg-color` | `#fafafb` | `#08080a` |
| `--headerbar-bg-color` | `#ffffff` | `#08080a` |
| `--headerbar-backdrop-color` | `#fafafb` | `#08080a` |
| `--pane-bg-color` | `#ffffff` | `#111114` |
| `--view-bg-color` | `#ffffff` | `#141418` |
| `--dialog-bg-color` | `#fafafb` | `#18181c` |
| `--popover-bg-color` | `#ffffff` | `#202024` |
| accent | desktop; default blue text `#0461be` / `#add1ff`, bg `#3584e4` | same desktop accent |

### Grove (hue 198, emerald-teal)

| Token | Light | Dark |
| --- | --- | --- |
| `--sidebar-bg-color` | `#dceaeb` | `#040f0f` |
| `--sidebar-backdrop-color` | `#e4f2f3` | `#040f0f` |
| `--window-bg-color` | `#ebf9f9` | `#020a0b` |
| `--pane-bg-color` | `#effdfd` | `#071414` |
| `--view-bg-color` | `#f3ffff` | `#0a1717` |
| `--dialog-bg-color` | `#f7ffff` | `#0e1b1b` |
| `--popover-bg-color` | `#fcffff` | `#162324` |
| `--accent-bg-color` | `#097e82` | `#097e82` |
| `--accent-fg-color` | `#ffffff` | `#ffffff` |
| `--accent-color` | `#076e71` | `#14bbc0` |

Accent-bg OKLCH L 0.539, C 0.090, h 198.8. Text light h 198.2, text dark h 198.3.

### Ocean (hue 228, cerulean)

| Token | Light | Dark |
| --- | --- | --- |
| `--sidebar-bg-color` | `#dde9ef` | `#050e12` |
| `--sidebar-backdrop-color` | `#e5f1f7` | `#050e12` |
| `--window-bg-color` | `#ecf8fe` | `#020a0e` |
| `--pane-bg-color` | `#f3fbff` | `#091318` |
| `--view-bg-color` | `#f8fdff` | `#0c161b` |
| `--dialog-bg-color` | `#fbfeff` | `#101a1f` |
| `--popover-bg-color` | `#feffff` | `#172227` |
| `--accent-bg-color` | `#097a9e` | `#097a9e` |
| `--accent-fg-color` | `#ffffff` | `#ffffff` |
| `--accent-color` | `#066b8c` | `#3fb3df` |

Accent-bg L 0.542, C 0.103, h 227.8. Text light h 228.5, text dark h 227.6.

### Ember (hue 46, burnt coral)

| Token | Light | Dark |
| --- | --- | --- |
| `--sidebar-bg-color` | `#f0e4df` | `#130a06` |
| `--sidebar-backdrop-color` | `#f8ece7` | `#130a06` |
| `--window-bg-color` | `#fff3ee` | `#0e0604` |
| `--pane-bg-color` | `#fff8f5` | `#180f0b` |
| `--view-bg-color` | `#fffbf9` | `#1b120e` |
| `--dialog-bg-color` | `#fffcfb` | `#201612` |
| `--popover-bg-color` | `#fffefe` | `#281e19` |
| `--accent-bg-color` | `#bc5107` | `#bc5107` |
| `--accent-fg-color` | `#ffffff` | `#ffffff` |
| `--accent-color` | `#a44505` | `#e28b62` |

Accent-bg L 0.565, C 0.155, h 46.3. Text light h 45.9, text dark h 45.6. Status overrides are in §3.

### Iris (hue 302, violet)

| Token | Light | Dark |
| --- | --- | --- |
| `--sidebar-bg-color` | `#e9e5f0` | `#0e0a13` |
| `--sidebar-backdrop-color` | `#f1edf8` | `#0e0a13` |
| `--window-bg-color` | `#f7f4ff` | `#09070e` |
| `--pane-bg-color` | `#fbf8ff` | `#131018` |
| `--view-bg-color` | `#fdfbff` | `#16131b` |
| `--dialog-bg-color` | `#fdfdff` | `#1a171f` |
| `--popover-bg-color` | `#fefeff` | `#221f28` |
| `--accent-bg-color` | `#895ac3` | `#895ac3` |
| `--accent-fg-color` | `#ffffff` | `#ffffff` |
| `--accent-color` | `#7a4bb3` | `#b392e3` |

Accent-bg L 0.566, C 0.160, h 302.2. Text light h 301.9, text dark h 302.2.

VTE foreground stays `#241f31` (light) and `#deddda` (dark). On these panes that is 15.18–15.97:1 light and 13.81–13.90:1 dark.

## 3. Status collisions

Meaning is fixed: amber warning = input, orange permission = approval, red error = failed, green success = done, accent = working or unread.

Stock standalone text (what `:root` already resolves, and what every theme except Ember keeps):

| Role | Light text | Dark text | Fill, light / dark |
| --- | --- | --- | --- |
| warning | `#825b00` h 79.6 | `#ffc252` h 79.7 | `#e5a50a` / `#cd9309` |
| permission | `#c64600` h 40.8 | `#ffa348` h 62.0 | ring `#ff7800` both |
| error | `#bc0015` h 26.6 | `#ffb9b3` h 24.5 | `#e01b24` / `#c01c28` |
| success | `#007748` h 157.6 | `#78e9ab` h 157.7 | `#2ec27e` / `#26a269` |

Dark error chroma is crushed to 0.082 by the `max(l, 0.85)` clamp, so `#ffb9b3` reads pale pink. It still passes contrast. Permission is not that clamp; the app uses the orange steps above.

### Grove

t3code's Grove orbs sit on the success hue (~157–159). The accent moves to 198.

| Pair | Δh | ΔE00 |
| --- | --- | --- |
| light accent text vs success text | 40.6 | 18.3 |
| dark accent text vs success text | 40.6 | 23.2 |
| dark accent text vs ANSI bright cyan `#4fd2fd` h 223.7 | 25.4 | 14.5 |

Success tokens stay stock. Done stays green; working and unread are teal.

### Ocean

GNOME blue and the usual Signal accent are h 255 (`#3584e4`, text `#0461be` / `#add1ff`). ANSI blue `#51a1ff` is h 254. The accent moves to 228.

| Pair | Δh | ΔE00 |
| --- | --- | --- |
| dark accent text vs `#add1ff` | 27.2 | 15.7 |
| dark accent text vs `#51a1ff` | 26.5 | 12.9 |
| accent-bg vs `#3584e4` | 27.5 | 14.2 |
| light accent text vs `#0461be` | 26.8 | 12.3 |
| Grove light text vs Ocean light text | 30.3 | 12.7 |

The ANSI pair is the weak one: pale and bright blues are low-chroma, so ΔE stays moderate while the hue gap is 26°. Tinted chrome carries the rest. Pushing Ocean toward 255 lands on the desktop blue. Pushing it toward 198 lands on Grove. Those two themes are never on screen together.

### Ember

The accent stays in the sienna band of the t3code orbs (h ~45–52). Warm status then has to spread, because accent, approval orange, and input amber cannot share one hue. Ember is the only theme that overrides status tokens. Success stays stock. `--destructive-*` stays stock: the new error fill is still red (ΔE 10.7 from `#e01b24`, ΔE 9.7 from `#c01c28`), and accent vs that fill is ΔE 23.3.

| Role | Hue | Light text | Dark text | Fill (both variants) | Fill fg |
| --- | --- | --- | --- | --- | --- |
| error | 16 | `#b13548` | `#e6848b` | `#d52f4f` | `#ffffff` |
| accent | 46 | `#a44505` | `#e28b62` | `#bc5107` | `#ffffff` |
| permission | 74 | `#875905` | `#d19845` | `#9b670b` | `#ffffff` on a filled chip |
| warning | 110 | `#666605` | `#a9ab4a` | `#76760b` | `#ffffff` |
| success | 158 | `#007748` | `#78e9ab` | stock | stock |

Tokens Ember sets: `--error-bg-color`, `--error-fg-color`, `--error-color`, `--warning-bg-color`, `--warning-fg-color`, `--warning-color`, `--permission-color`, `--permission-bg-color`. Warning fg must be white because the fill is dark (white on `#76760b` is 4.81:1); stock black-80% warning fg is wrong on it. Do not set `--destructive-*`. Do not retint `--orange-*`. Point the permission pill, row, and attention ring at `--permission-color` / `--permission-bg-color` instead of `--orange-5` / `--orange-2` / `--orange-3`.

| Pair | Light Δh | Light ΔE00 | Dark Δh | Dark ΔE00 |
| --- | --- | --- | --- | --- |
| accent vs error | 29.9 | 21.2 | 29.6 | 16.2 |
| accent vs permission | 27.7 | 14.1 | 28.3 | 14.7 |
| permission vs warning | 36.1 | 16.3 | 36.3 | 18.3 |
| warning vs success | 47.8 | 22.1 | 47.5 | 24.8 |

Accent vs permission is the closest pair. The hue gap is what separates them. Warning cannot stay at 80: that slot is between permission (74) and the accent. Hue 110 is the first mustard that clears ΔE 16 against permission and still reads as yellow. It is Δh ~48 and ΔE ~22–25 from success, so it does not become "done".

White on the three Ember fills is 4.83, 4.84, and 4.81. Non-text contrast of those fills against both Ember panes is ≥ 3.90. A 16% pill wash of the fill over the light pane `#fff8f5` still clears 4.5 for the light text: error wash `#f8d8da` 4.56, permission `#efe1d0` 4.72, warning `#e9e3d0` 4.72.

### Iris

No status retint. The collision is the workspace marks in §4.

Unread-dot non-text contrast of accent-bg on the dark pane is 3.83–3.88, above the 3:1 non-text bar. It is not in the 4.5 table.

## 4. Workspace marks

Current slots, chosen to avoid status hues and blue:

| Slot | Light | Dark | Hue |
| --- | --- | --- | --- |
| tint-0 | `#613583` on `alpha(#9141ac, 0.22)` | `#dc8add` on `alpha(#9141ac, 0.70)` | 307.8 / 326.5 |
| tint-1 | `#63452c` on `alpha(#986a44, 0.25)` | `#cdab8f` on `alpha(#b5835a, 0.55)` | 60.2 / 61.3 |
| tint-2 | sRGB mix of tint-0 and tint-1 | sRGB mix of the dark pair (fill mixes `--purple-4` / `--brown-3`) | 337.3 / 348.6 |
| tint-3 | `alpha(currentColor, 0.8)` on `alpha(currentColor, 0.12)` | `#f6f5f4` on `alpha(#deddda, 0.22)` | chroma under 0.02 |

tint-2 exists only as the mix of tint-0 and tint-1. Replacing a parent replaces the mix.

Collision rule. A slot collides when its glyph chroma is ≥ 0.04 and, against that theme's accent text of the same variant or against that theme's warning, permission, error, or success text, either Δh ≤ 18° or ΔE00 ≤ 14. tint-3 never collides. tint-2 is replaced whenever either parent is replaced, and the replacement is a spare hue rather than a new mix. Spares are teal 198 and steel 250. A spare is skipped if it would itself collide. Signal keeps all four hues. If the desktop accent is purple, tint-0 collides, and if it is brown, tint-1 collides. Signal cannot know the accent, so that residual stays.

| Theme | tint-0 | tint-1 | tint-2 | Why |
| --- | --- | --- | --- | --- |
| Signal, Grove, Ocean | violet | umber | rose | No parent collides. Grove vs violet/umber is far from h 198. Ocean vs them is far from h 228. |
| Ember | violet | teal | steel | Umber vs accent is Δh 14.3 / 15.8 (ΔE 16.6 light, 13.1 dark) and vs permission text Δh 12.4–13.5 (ΔE 14.2 / 13.3). |
| Iris | teal | umber | steel | Violet vs accent is Δh 5.9 and ΔE 10.0 light, Δh 24.3 and ΔE 9.6 dark. |

Iris tint-2's mixed hue does clear the accent on its own (Δh 35–47, ΔE above 16). It still goes, because it is defined as the mix of the colliding violet parent. Ember tint-2's rose also clears the accent (Δh ~56) but is the closest non-accent pair to Ember's dark error text (Δh 27.3, ΔE 12.4), which is under the ΔE 14 floor.

Dark glyphs at the current alphas fail on Signal's sidebar `#0c0c0e`: tint-0 3.73, tint-1 3.61, tint-2 3.56. Every dark theme therefore uses a lighter glyph (L ≈ 0.82, C ≈ 0.12) on `alpha(fill, 0.45)`, keeping the hue. tint-3 stays as it is. Minimum contrast across Signal, Grove, Ocean, Ember, and Iris sidebars:

| Hue | Glyph | Fill at α 0.45 | Min contrast |
| --- | --- | --- | --- |
| violet, kept tint-0 | `#eda9ee` | `#623264` | 8.39 |
| umber, kept tint-1 | `#fdb270` | `#6a3a02` | 8.48 |
| rose, kept tint-2 | `#fea4d0` | `#6d2e50` | 8.35 |
| teal spare | `#4cdce0` | `#035255` | 8.87 |
| steel spare | `#96c9fe` | `#124a7b` | 8.52 |
| slate tint-3 | `#f6f5f4` | `alpha(#deddda, 0.22)` | 10.33 |

Light kept slots stay stock. Minimum on the five light sidebars: tint-0 5.33, tint-1 5.26, tint-2 5.28, tint-3 5.32. Signal's own sidebar is 5.56 / 5.45 / 5.51 / 5.42.

Light spares, both at alpha 0.22. An earlier pair (`#08787b` on `#89c4c5`, `#116bb5` on `#9bbbdd`) was only ~5:1 on white and fell to ~3.4–4.1 once the wash sat on a real sidebar. These are darker:

| Hue | Glyph | Fill | Worst sidebar |
| --- | --- | --- | --- |
| teal | `#005355` h 197.6 | `#1d7779` | 5.39 on Ember `#f0e4df` (Iris 5.42) |
| steel | `#004f8b` h 250.0 | `#4c759f` | 5.26 on Iris `#e9e5f0` (Ember 5.27) |

Teal and steel were checked against Iris and Ember accent and status text: no pair is inside Δh 20 or ΔE 14.

## 5. Contrast

Required floors: body text ≥ 4.5, accent text on canvas and on the pane ≥ 4.5, white on accent-bg ≥ 4.5, each status text on the pane ≥ 4.5. All of those pass except Signal's accent pair, which is the desktop blue called out in §1.

| Theme | Variant | Fg / window | Fg / pane | Fg / sidebar | Accent / window | Accent / pane | White / accent-bg |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Signal | light | 12.22 | 12.56 | 11.18 | 5.82 | 6.07 | 3.77 |
| Signal | dark | 20.01 | 18.85 | 19.54 | 12.71 | 11.97 | 3.77 |
| Grove | light | 11.92 | 12.17 | 10.88 | 5.59 | 5.79 | 4.86 |
| Grove | dark | 19.98 | 18.76 | 19.44 | 8.47 | 7.95 | 4.86 |
| Ocean | light | 11.89 | 12.20 | 10.85 | 5.57 | 5.75 | 4.90 |
| Ocean | dark | 19.96 | 18.78 | 19.48 | 8.29 | 7.80 | 4.90 |
| Ember | light | 11.85 | 12.12 | 10.81 | 5.63 | 5.83 | 4.86 |
| Ember | dark | 20.07 | 18.88 | 19.56 | 7.74 | 7.29 | 4.86 |
| Iris | light | 11.89 | 12.11 | 10.83 | 5.55 | 5.73 | 4.87 |
| Iris | dark | 20.03 | 18.83 | 19.60 | 7.76 | 7.30 | 4.87 |

Accent text on the sidebar is not required. The tightest light cell is still ≥ 4.86 (Ocean and Iris).

Status text on the pane. Ember uses its own warning, permission, and error text. Everyone else uses the stock column.

| Theme | Variant | Warning | Permission | Error | Success |
| --- | --- | --- | --- | --- | --- |
| Signal | light | 6.09 | 4.91 | 6.67 | 5.63 |
| Signal | dark | 11.75 | 9.50 | 11.53 | 12.60 |
| Grove | light | 5.84 | 4.71 | 6.40 | 5.40 |
| Grove | dark | 11.70 | 9.46 | 11.47 | 12.55 |
| Ocean | light | 5.82 | 4.69 | 6.37 | 5.37 |
| Ocean | dark | 11.71 | 9.47 | 11.49 | 12.56 |
| Ember | light | 5.77 | 5.78 | 5.76 | 5.36 |
| Ember | dark | 7.74 | 7.45 | 7.23 | 12.63 |
| Iris | light | 5.80 | 4.67 | 6.34 | 5.35 |
| Iris | dark | 11.74 | 9.49 | 11.52 | 12.59 |

The lowest required coloured cell is white on accent-bg, 4.86–4.90. The lowest status cell is Iris light permission, 4.67, which is the stock `#c64600` on a near-white pane.

## 6. High contrast

`.high-contrast` still has to win after a theme. Theme rules that set `--border-color` or `--headerbar-shade-color` must not outrank it: put the high-contrast block last, and keep the selectors at least as specific as `.theme-*.dark` (two classes). A concrete theme alpha otherwise beats both `--border-opacity` and a one-class `.high-contrast` rule.

Set, for both schemes:

- `--border-color: alpha(currentColor, 0.55)` and `--border-opacity: 50%`
- `--headerbar-shade-color: alpha(currentColor, 0.55)` (today this exists only as `.dark.high-contrast`, and themes set the shade to `transparent`)
- `--sidebar-border-color: alpha(currentColor, 0.55)`

Keep the rules already in `style.css`: secondary text (`.workspace-message`, `.workspace-meta`, `.sidebar-section`, `.row-status`) returns to `inherit`; `.pane-subtitle` opacity returns to 1; `.pane-header` keeps a 1px `currentColor` hairline; a focused pane keeps the 3px accent bar; `.status-pill` and `.ws-mark` keep a 1px `currentColor` ring.

Do not flatten the theme accent back to desktop blue. Do not retint ANSI. Do not encode dimmed secondary text as a weaker fg token; high contrast restores it by inheritance and opacity. The dark card film can stay.

## 7. Decision / Rationale / Alternatives considered

### Decision

Ship Signal as the current neutral look, Grove at hue 198, Ocean at 228, Ember at 46, Iris at 302. One accent-bg per theme, white on it, and a separate accent text per variant. Dark surfaces copy Signal's shipped lightness order and take chroma 0.018. Light surfaces are a soft tint at chroma 0.015 with the sidebar as the low step. Ember alone retints error to 16, permission to 74, and warning to 110. Workspace marks move only on Ember (tint-1, tint-2) and Iris (tint-0, tint-2), and every dark mark glyph gets lighter so it clears 4.5:1.

### Rationale

`docs/14-product-direction.md` §7 asks for semantic canvas, surface, border, text, and accent, with elevation from lightness plus a 1px hairline, still expressed through libadwaita. Tinting the existing variables does that. The terminal stays readable because VTE foreground cannot change and already clears 13:1 on these panes. Status colour is a vocabulary (input, approval, failed, done, working); a theme hue that lands on one of those words has to move, or the status has to move. Grove and Ocean move the accent. Ember cannot move the accent out of the coral band without ceasing to be Ember, so the three warm statuses spread and success stays green. Iris keeps a violet accent and moves the violet mark. Signal stays byte-for-byte on the dark surfaces already shipped, including the window-darker-than-sidebar order the comment does not describe.

### Alternatives considered

- Tint the ANSI palette or the VTE foreground. Rejected. The palette is shared with the shell the user already has, and the brief keeps it fixed.
- Retint GNOME `--orange-1`…`--orange-5`. Rejected. Adwaita widgets outside permission use that ramp. Ember overrides two app tokens instead.
- Follow the CSS comment and make the sidebar the darkest plane. Rejected. "Signal dark = the current values exactly" is the constraint. The comment is wrong about the shipped hexes.
- Maximise ΔE and let Ember's error go to hue 0 and warning to hue 103. Rejected. Error became pink and warning became yellow-green. The hue gap is kept only where the role is still the right word.
- Warning near hue 98. Rejected. It stayed around ΔE 11–12 from permission.
- Neon dark accent text at max chroma (`#4ff8fd`, `#43cafe`, and the same family). Rejected. At L ≈ 0.85 those read as laser light. Dark accent text is capped at chroma 0.12 and is the darkest L that still clears about 5.2:1.
- One accent text hex for both variants. Rejected. A colour that is dark enough for a light canvas is invisible on a dark one, and the reverse fails on white.
- Let `:root`'s `oklab(from var(--accent-bg-color) …)` recompute `--accent-color` when a theme class overrides the bg. It does not. The standalone colour is set on `:root`.
- "Fix" Signal's 3.77:1 blue by authoring accent variables. Rejected. Signal's accent is the desktop accent on purpose.
