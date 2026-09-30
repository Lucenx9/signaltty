# Workspace visual hierarchy verification — 2026-09-30

Scope: [spec 012](../../specs/012-workspace-visual-hierarchy/spec.md).
Baseline: `97d9aadc07c9ee988e0d37432b71514a7fa72fcd`.
Real GTK/libadwaita/VTE client on KDE Plasma with isolated servers and PTYs;
deterministic agent/approval fixtures, no paid provider requests.

| Before | After | Why |
| --- | --- | --- |
| Close reserves a column across all three sidebar lines | Reserved only on the name/status line; message and metadata use full width | More useful text fits without widening the sidebar |
| Dense row spacing and faint metadata | More line spacing and stronger secondary text | Names, activity and location scan more clearly |
| Header blends into terminal output | Quiet native surface and inset separator | Pane controls are visually separate from output |
| Inactive titles inherit extra opacity | All titles remain readable; focused header has a subtle accent wash | Focus does not cost legibility, even with an attention ring |
| Long directory precedes branch | Branch first, full context in tooltip | Useful workspace context is less likely to be truncated |

## Native evidence

All captures were inspected. The fixture checks the owned GUI PID, active window
and actual width before screenshotting. Temporary directory names are fixture data.

- [Baseline light](evidence-visual-hierarchy-2026-09-30/before-light.png)
- [Refined light](evidence-visual-hierarchy-2026-09-30/after-light.png)
- [Refined dark](evidence-visual-hierarchy-2026-09-30/after-dark.png)
- [360px light](evidence-visual-hierarchy-2026-09-30/narrow-light.png),
  [360px dark](evidence-visual-hierarchy-2026-09-30/narrow-dark.png)
- [High contrast](evidence-visual-hierarchy-2026-09-30/high-contrast.png)
- [Sans 18](evidence-visual-hierarchy-2026-09-30/large-font.png)

Large fonts deliberately ellipsize long sidebar labels with full-text tooltips;
status and approval choices remain visible. Narrow scenes retain the existing
overlay sidebar and terminal line wrapping.

Reproduce with `cargo build --workspace`, then:

```sh
ADW_DEBUG_COLOR_SCHEME=prefer-light python3 scripts/qa-ui-scenes.py --output /tmp/ui-light.png
ADW_DEBUG_COLOR_SCHEME=prefer-dark python3 scripts/qa-ui-scenes.py --output /tmp/ui-dark.png
ADW_DEBUG_COLOR_SCHEME=prefer-light python3 scripts/qa-ui-scenes.py --width 360 --long-choice --output /tmp/ui-narrow.png
ADW_DEBUG_HIGH_CONTRAST=1 python3 scripts/qa-ui-scenes.py --output /tmp/ui-contrast.png
python3 scripts/qa-ui-scenes.py --font 'Sans 18' --output /tmp/ui-font.png
```

The desktop accessibility socket was stale during this run. Captures used a
temporary dedicated accessibility bus and registry through `AT_SPI_BUS_ADDRESS`;
the user's desktop preferences and session server were untouched.

## Verification and review

[Validation](evidence-visual-hierarchy-2026-09-30/validation.json) records passing
workspace build, fmt, Clippy, 211 workspace tests and 11 graphical tests. Clippy retains the two
pre-existing server warnings in `audit.rs` and `router.rs`; no new warnings.
Graphical regressions are recorded separately in
[display-tests.jsonl](evidence-visual-hierarchy-2026-09-30/display-tests.jsonl).
They cover narrow approval allocation and answer IDs, mounted VTE retention,
keyboard-visible controls, pointer/reduced-motion behavior, workspace closure,
async refresh/navigation and setup notices in both themes.

Independent standards and specification reviews found no code defects; the
standards review caught outdated prose about dimming, corrected in `docs/06`.
No new custom motion: frequency gate rejects navigation/focus animation. Existing
pointer feedback remains a 120ms transform, scale 0.97, strong ease-out
`cubic-bezier(0.23, 1, 0.32, 1)`, disabled for reduced motion/keyboard activation.

The design follows [GNOME styling guidance](https://developer.gnome.org/hig/guidelines/ui-styling.html)
and [sidebar guidance](https://developer.gnome.org/hig/patterns/nav/sidebars.html),
checked on 2026-09-30, together with the invoked design skills. This verification
covers presentation and existing GUI behavior, not new provider integrations.
