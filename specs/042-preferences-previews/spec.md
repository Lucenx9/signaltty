# Specification: Preferences previews

**Branch**: `gui/preferences-previews` | **Created**: 2026-10-10
**Status**: Implemented and verified
**Input**: Take the polish of t3code's Appearance settings (color-scheme window previews, theme orbs) into the Preferences dialog.

## User scenarios and testing

### User story 1: See what a scheme looks like before choosing it (P1)
Acceptance: System, Light and Dark each show a miniature window: a sidebar, content with a title and text lines, a side card and an input bar with the accent dot. System splits the content down the middle, light on the left and dark on the right. The previews use literal colours, so a dark window still shows Light faithfully and the reverse.

### User story 2: Recognise a theme at a glance (P1)
Acceptance: Each theme card shows two orbs: the light canvas lit from the top left fading into the accent, and the accent glowing out of the dark canvas. Choosing a card applies and persists the theme exactly as before.

### User story 3: Selection reads as a ring (P2)
Acceptance: The selected scheme and theme carry the accent ring only. libadwaita's checked-card tint is removed so it cannot recolour the preview it frames; hover keeps a faint tint.

## Requirements
- FR-001: `scheme_preview(Appearance)` builds the miniature from boxes and CSS classes (`scheme-preview`, `mw-*`); no images or drawing code.
- FR-002: Each preview is backed by its own colours (System: a `calc(21px + 50%)` split matching its halves), so antialiasing at fractional part boundaries cannot let the card colour draw hairlines.
- FR-003: No new preferences, persistence or dependencies.

## Success criteria
GTK test `preferences_preview_schemes_and_themes` checks three previews of at least 148×92, the System split, ten orbs, that a theme click applies and persists, and that one scheme and one theme stay selected. It also captures light and dark. `scripts/verify.sh full` passes.
