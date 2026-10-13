# Verification

Run `scripts/verify.sh doctor`, targeted core project, server worktree and native
GTK project tests, then `scripts/verify.sh full --output /absolute/new-directory`.
All fixtures own their socket, state and config; never use the user's server.

Set `SIGNALTTY_UI_EVIDENCE=/tmp/project-sidebar-renders` for native captures. Repeat
`app::tests::project_scope_filters_without_rebuilding_workspaces` in its own Xvfb
and D-Bus process with `ADW_DEBUG_HIGH_CONTRAST=1`. Inspect both window and popover
captures: GTK popovers use a separate native surface and are captured directly.
Review light/dark, 360px, Sans 18, long/duplicate names and RTL. Read docs 17 for
proof limits. Final review: independent Standards and Gemini 3.8 Flash High Spec/UX,
with Antigravity full-access/default mode as authorized by the user.
