# Top bar design research

## Primary web sources
[GNOME header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html), accessed 2026-10-13: few contextual controls, start/title/end alignment, blank space for dragging, tooltips and spacing to distinguish groups rather than linked boxes.
[Apple toolbars](https://developer.apple.com/design/human-interface-guidelines/toolbars), accessed 2026-10-13: frequent commands should remain available, secondary commands belong in overflow, and mixed labels/symbols need enough separation to avoid looking like one combined action. The page requires JS through the text fetcher; the primary-site indexed excerpt supplied those points.
Installed Adw GIR for HeaderBar.title-widget and CenteringPolicy explains native title allocation and loose versus strict centering. T3 reference checkout remains 740bda40c9, with its compact, semantically grouped controls.

## Decision and alternatives
Prefer the native title-widget lane over packing identity as another leading action. It allows identity to use remaining width and groups creation/navigation independently of views. Keep the established labels, symbols, 28px minimum/native targets and semantic colors. The Details button remains visually quiet and contains full metadata; a real non-interactive spacer provides a drag region. Hide the decorative mark and secondary context in compact content. Relocate New Tab, Board and Changes to labeled sidebar controls at that width; menus and shortcuts retain access.
Alternatives: rigid centering would waste space when trailing controls are wider; shrinking all fonts would damage enlarged text; removing New Tab or custom window chrome would weaken discoverability/native behavior. No new custom widget or resize polling is warranted.

## Tokens and critique
Current canvas/surface/border/text/accent roles and system font remain. Primary title semibold, context 0.88em secondary, flat action buttons and 6px geometry. Use small gaps within action groups, a larger gap across groups, no new boxed toolbar panels or decorative animation. Spend the change on identity allocation and functional grouping, not a new visual theme.

## Native allocation findings
The baseline long-name title measured 31px at Sans11 and 25px at Sans18 in the narrow fixture. Concealing the attention Revealer still reserved 35px; direct button visibility removes that footprint. The final no-attention title measures 126px in both font fixtures, and 73px with active attention and enlarged text. Actual content-width breakpoints keep icons compact when a wide window has a docked Changes panel; global window width cannot express that constraint. The hidden content bin can retain old allocations, so compact footer visibility joins sidebar collapse and header creation visibility with one writer.

## Full-width correction, 2026-10-14

Rechecked [GNOME header-bar guidance](https://developer.gnome.org/hig/patterns/containers/header-bars.html) on 2026-10-14: preserve blank drag space and communicate relationships through spacing. Apple toolbar page still requires JavaScript; no new claim is based on its unreadable body. The earlier native title-widget preference did not meet the user's wide-window requirement: an expanding title label split the identity, and native center allocation added a leading gap. Final placement uses native pack_start after navigation, with an empty center title widget. Only the drag spacer expands. No custom chrome is introduced.
