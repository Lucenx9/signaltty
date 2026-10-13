# Research and design critique

## Primary guidance, accessed 2026-10-14
[GNOME app icons](https://developer.gnome.org/hig/guidelines/app-icons.html): unique domain-related metaphor; simple geometric 128px artwork with breathing room, flat colors for flat surfaces, subtle bottom profile, reduced detail at 32/64, matching symbolic variant. [GNOME header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html): few controls, quiet flat appearance, group by spacing, retain blank native drag space.
[Apple app icons](https://developer.apple.com/design/human-interface-guidelines/app-icons?changes=latest_maj_4_7&language=objc): simple unique core concept, few shapes, consistent identity across contexts, avoid detail lost at small sizes. This primary indexed page supplies content while the canonical text fetch requires JavaScript. [Apple branding](https://developer.apple.com/design/human-interface-guidelines/branding): use branding throughout the experience with restraint.
Apple/Emil/frontend-design skills support consistent geometry, hierarchy through weight, system text scaling and concentrating distinctive treatment in one element. Web motion/material examples are not applied to a static native header.

## Alternatives
A larger colored logo tile would compete with workspace marks and attention. An oversized wordmark or descriptor consumes the constrained rail. A branding button would suggest an unrequested Home/About interaction. A tiny framed terminal repeats a generic launcher metaphor. Choose a static open prompt with two parallel rails: terminal input plus parallel agent work. Reuse its silhouette across UI and launcher rather than adding AI sparkles or provider marks.

## Tokens and planned layout
Native canvas/surface/text/accent remain in the UI. Launcher-only palette: profile #171b24, body #292f3d, quiet rim #41495a, prompt #f4f5f7, rail #b5a1d9. System font, weight600, ordinary text size with neutral tracking. Native header height 40px retained. Small mark 20px, 8px gap, logical leading alignment; New Workspace remains trailing. The lockup has no hover/click state and no motion.

```text
[ prompt/rails  signaltty ]          [ + ]
[ search commands                     ]
[ project scope                       ]
```

The restraint critique removed the colored UI tile and secondary tagline. The UI mark is monochrome; only desktop artwork uses the brand color. At narrow rail widths the name can ellipsize before sacrificing the creation target, with a full tooltip.
