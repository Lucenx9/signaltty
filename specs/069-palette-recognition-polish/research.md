# Research

T3 Code CommandPaletteResults.tsx at `50647de0143e68fde051eb1a0ccf4fe63f9947b2` renders an item icon, label/description, then a subordinate shortcut. Translate this to native ActionRow prefix/title/subtitle/suffix. This is our application of its hierarchy, not copied artwork.

GNOME UI icon guidance, read 2026-10-15: https://developer.gnome.org/hig/guidelines/ui-icons.html . Symbolic artwork is monochrome on a 16px grid and labels remain safer than unfamiliar icon-only actions. We retain every command label and use existing app-owned category glyphs, avoiding host-theme dependencies or new artwork. Existing sidebar marks supply stable workspace identity. No new library or runtime choice.
