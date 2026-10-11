# Verification — 2026-10-11

Red evidence: preferences fixture failed (0 miniatures, expected 10), board fixture
failed (title not wrapped), navigation fixture failed waiting for group header.
Logs: `/tmp/secondary-red.log`, `/tmp/secondary-board-red.log`,
`/tmp/secondary-navigation-red.log`. Behavior assertions preceded source edits.

Full gate: **passed**, 47 steps including all 35 isolated GTK tests, at `/tmp/secondary-window-verification-complete`, clean
revision `5e9e20153ed153c20cc0c0afe66f0497b2a88dc5`. Previous terminal runners
were externally interrupted during build/tests; neither is passing evidence.
Initial narrow render found clipped navigation labels; replaced the horizontal
box with native wrapping. Worktrees' own content bounds, not the dialog overlay
allocation, measure the compact body. The navigation fixture now loads production
CSS, fixing the old unstyled palette/Changes/Worktrees captures.

Not run: real desktop screen reader, on-screen keyboard, IME, folder chooser,
notification clicks and paid-provider sessions. Isolated high contrast uses the
application class; it is not desktop accessibility certification.

## Native visual acceptance

Selected full-gate captures live in
[the evidence directory](../../docs/qa/secondary-window-polish-2026-10-11/).
Direct isolated fixtures passed: preferences, board narrow navigation, desktop
board activation and palette/Git workflows. Independent Standards and Spec source
reviews found zero remaining issues. Both reviewers accepted final visual scenes.

| Surface | Before | After |
| --- | --- | --- |
| Board | One-line title shares width with state | Two-line full-width title; overflow-only Previous/Next controls |
| Worktrees | Tall reserved body; path is title | Compact natural body; branch above path and availability |
| Palette | Mixed list | Filter-aware headings; unchanged keyboard targets |
| Preferences | Color orbs | Paired application miniatures; wrapping choices |
| Changes | Quiet directory labels | Semantic separators and clearer directory text |

Palette, Worktrees and Changes old captures lacked production CSS, so they are
not used as before/after evidence of the actual product. Final captures load it.

[Complete passed run record](../../docs/qa/secondary-window-polish-2026-10-11/summary.json).
Workspace tests, warning policy, build, server QA, isolated GTK tests and refresh
benchmark all passed. Only documentation and selected evidence were added after
the clean application revision above; application source is unchanged.
