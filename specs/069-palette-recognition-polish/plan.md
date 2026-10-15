# Implementation plan

## Design and review
Apply frontend-design, emil-design-eng, apple-design and karpathy-guidelines. Existing system sans, primary titles and secondary path/shortcut roles; current canvas/view/text/border/accent tokens stay. Signal dark examples remain canvas #090909, view #111113. Workspace prefixes reuse sidebar::mark/set_mark. Command glyphs are 16px in a 20px prefix lane, neutral currentColor at existing secondary contrast; high contrast restores full foreground. No command badge background.

T3 Code CommandPaletteResults.tsx at 50647de0143e68fde051eb1a0ccf4fe63f9947b2 places item icons before title/description and shortcut afterward. Translate that sequence to native ActionRow prefixes and suffixes. Existing groups/search/activation remain. GNOME symbolic icon guidance supports simple recognizable monochrome glyphs: https://developer.gnome.org/hig/guidelines/ui-icons.html . Do not copy T3 assets or web palette.

Review against brief: workspace marks connect the same entities across surfaces; command category icons reinforce labels without making every row a new colored badge. Keep native widgets and no new assets/dependencies. A secondary command icon should not reduce shortcut legibility; verify real narrow/enlarged allocations before acceptance.

## Technical design
Add bundled icon metadata to the existing action registry. Palette Target::Action references its static ActionDef instead of storing a formatted name; both icon and activation identity come from one registry row. Build one decorative prefix per native ActionRow; workspace rows retain their ID target and use the shared mark. Existing ActionRow logical prefix placement handles RTL. Add only scoped icon CSS.

## Sequence and checks
Extend existing native palette tests first and demonstrate red for missing prefixes. Verify workspace mark/tint, command glyph roles, decorative accessibility role, prefix geometry, existing shortcut/selection/target mapping, RTL and all action icon resolution without external themes. Implement, inspect desktop light/dark/narrow/large/high-contrast/RTL captures. Independent Spec/Standards review, durable evidence and docs precede source freeze. Full and fresh delivery proof before publish/link; latest CI/reviews before authorized merge.

## Constitution and contracts
Rust 1.94 development/minimum 1.92, existing GTK4/libadwaita. No domain model, persisted data or IPC contract changes; UI registry remains toolkit-facing. No load-bearing architecture choice. Display tests necessary for allocation and native accessibility roles; manual AT/IME not certified by Xvfb. All principles satisfied.
