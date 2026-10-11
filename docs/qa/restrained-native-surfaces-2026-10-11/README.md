# Restrained native surfaces, 2026-10-11

Selected production GTK renders from `scripts/verify.sh full --output /tmp/restrained-native-verification-final` at clean `8bf1d3e57ab15bb5e833bfc9bdc77799e938d3ac`.
All 47 verification steps passed, including 35 isolated GTK tests. See `summary.json` and the feature verification document for scope and limits.

Preferences, palette, Worktrees, Board, New Workspace and details were reviewed in light/dark. Narrow and enlarged high-contrast captures retain explicit focus, selection and state navigation. The details fixture captures the native popover directly because a window snapshot excludes its separate surface.

These are fixture scenes using production widgets and CSS. Placeholder IPC responses in the New Workspace scene produce a Changes error in the background; this capture verifies the dialog. Xvfb captures do not certify desktop screen reader, IME, chooser or real-provider integration.
