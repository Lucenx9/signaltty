# Verification — 2026-10-11

Red evidence: preferences fixture failed (0 miniatures, expected 10), board fixture
failed (title not wrapped), navigation fixture failed waiting for group header.
Logs: `/tmp/secondary-red.log`, `/tmp/secondary-board-red.log`,
`/tmp/secondary-navigation-red.log`. Behavior assertions preceded source edits.

Full gate: in progress at `/tmp/secondary-window-verification`.
Initial narrow render found clipped navigation labels; replaced the horizontal
box with native wrapping. Worktrees' own content bounds, not the dialog overlay
allocation, measure the compact body. The navigation fixture now loads production
CSS, fixing the old unstyled palette/Changes/Worktrees captures.

Not run: real desktop screen reader, on-screen keyboard, IME, folder chooser,
notification clicks and paid-provider sessions. Isolated high contrast uses the
application class; it is not desktop accessibility certification.
