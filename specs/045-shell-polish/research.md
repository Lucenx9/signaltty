# Research and visual direction — 2026-10-11

Sources inspected (a curated sample, not an objective ranking of Rust apps):

- [T3 Code](https://github.com/pingdotgg/t3code) and its [stylesheet](https://github.com/pingdotgg/t3code/blob/main/apps/web/src/index.css): compact semantic geometry, quieter navigation, restrained hover/selected surfaces.
- [Linear's March 2026 refresh](https://linear.app/now/behind-the-latest-design-refresh): predictable header actions, less prominent navigation, fewer decorative borders. Inspected the actual sidebar comparison artwork.
- [Vercel Geist typography](https://vercel.com/geist/typography) and [badges](https://vercel.com/geist/badge): deliberate text hierarchy and concise status terminology. Preserve the Linux system font rather than bundling Geist.
- [Zed](https://zed.dev), [Lapce](https://lap.dev/lapce/) and [COSMIC](https://system76.com/cosmic): editor/content density, compact tools and native theme adaptation. Inspected official product imagery for Zed, Lapce and COSMIC; these are visual references, not proposed toolkit migrations.

## Decision

Compare the three sketches in `exploration.html`: a compact icon rail with
workspace switcher, a floating dashboard of project cards, and a persistent
attention sidebar with restrained chrome. Choose the persistent sidebar: it
keeps all agent requests visible and fits the existing native interaction model.
The rail hides simultaneous status; the dashboard consumes output space and
introduces a second navigation model. Neither earns that cost in a polish pass.

Color roles remain libadwaita canvas/sidebar/view/foreground/accent, plus existing
semantic status colors. Signal dark retains #0c0c0e sidebar, #16161a canvas and
#111114 pane; light retains desktop values. Type stays system sans with existing
1em names, .9em messages and .82em metadata. Left alignment, 8px control radius,
10px pane radius and inset selection preserve the native spatial model.

| Before | After | Why |
| --- | --- | --- |
| Every speaking status gets a fill | Fill only for waiting/failed/attention | Progress does not compete with decisions |
| Selected row uses only a subtle slab | Slab plus inset leading marker | Selection has a shape cue |
| Board/Changes have borders at rest | Flat tools; outlined active Changes | Reserve chrome for actual state |
| Two drop shadows per terminal card | One restrained shadow | Output carries the visual weight |
| “No Workspaces” / “No Tabs” | Action-led copy and shortcut hints | Explain the next step |

Apple design: platform typography, spatial consistency, immediate feedback and
agency. Emil design engineering: restraint for frequent interactions; no new
animation. Frontend design: attention is the distinguishing element, not generic
dashboard decoration. No new load-bearing choice; existing GTK ADRs apply.
