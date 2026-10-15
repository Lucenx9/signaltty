# Verify terminal interactions

Use the isolated GTK tests and [project verification guide](../../.agents/skills/verify-signaltty/SKILL.md). Keep the user's desktop session and server separate from test fixtures.

1. Send Ctrl+Space, a control key and Unicode text through a live terminal. Inspect the server-bound bytes, including the single NUL for Ctrl+Space. Repeat after marking the pane exited and confirm no new input.
2. Select known terminal output. Copy with the terminal shortcut and menu, then read the desktop clipboard. Paste Unicode and multiple lines into a program that enables bracketed paste. Check its received bytes and the absence of an added Enter.
3. Print a history marker and enough lines to scroll it offscreen. Reconnect the GUI to the same running pane twice. Confirm the history marker occurs once, the current screen matches the server, and replay adds no duplicate visible marker. Repeat with an alternate-screen program.
4. Open two panes. Increase and decrease the text size in one, change the theme, and reset the scale. Confirm the other pane and both terminal widget identities remain unchanged.
5. Inspect terminal menus in light and dark. Check keyboard invocation, disabled Copy without selection, disabled Paste for an exited pane, focus restoration, narrow width and readable zoom labels.

Run `scripts/verify.sh full` on staged, frozen source. Inspect the emitted native renders and check delivery proof for `070-vte-terminal-interactions`. Xvfb does not certify IME, real Wayland clipboard transfers or assistive technology behavior.
