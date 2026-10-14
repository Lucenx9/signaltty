# Implementation plan

Read docs06/14/16/17/18/19, constitution and requested design/implementation skills. Spec first, clarified accepted scope, then tasks before code.

## Design
Use current canvas/sidebar/view/border/text/semantic diff color roles, no copied web palette. Signal dark examples are canvas #090909, view #111113, primary #ededed, secondary derived from currentColor; green/red remain existing semantic tokens. System sans for controls, desktop monospace for diff. A stable Changes header precedes a full-width native scope row. File path stays primary metadata with comparison context secondary. Hunk bands and spacing establish sections; number prefixes use the same font metrics as code with quieter foreground.

Native structure: NavigationView → list page [HeaderBar title / ToggleGroup / summary / file list]; reader page [native back/refresh / file path / scope/notices / one TextView]. No new widget abstraction or dependency. Existing blank/native spacing, action names, state and request generations remain.

## Sequence and validation
1. Capture baseline through existing display tests.
2. Extend existing scope/reader native tests with visible labels, narrow/enlarged/RTL fit and hunk geometry; reproduce red.
3. Move scope selector below native title, style existing TextTags for numbers/hunks and metadata spacing. Preserve copied text and native scrolling.
4. Run exact interaction and native stress tests; inspect real captures. Update GUI doc and feature map.
5. Independent Spec/Standards review, selected durable native evidence/reproduction instructions. Stage all source/artifacts/tasks before final full run; fresh delivery gate binds exact content/index. Keep final execution report and handoff outside checkout so proof stays fresh. Publish/link PR with actual source/proof and CI pending.

## Alternatives and limits
Do not add GtkSourceView or custom gutters for this polish: extra dependency/custom copy paths are unnecessary. Separate TextViews would complicate synchronized scrolling/selection. TextTag paragraph spacing and foreground preserve one buffer. Do not assert colors from standalone grey test scenes represent the app theme; inspect App scenes for application styling too. No manual desktop certification claimed.
