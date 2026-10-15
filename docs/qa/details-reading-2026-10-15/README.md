# Workspace Details reading polish

Native GTK captures on 2026-10-15. The baseline is main `6c6234d`; Details artwork is unchanged from the preceding full verification. After images come from the extended `breadcrumb_opens_the_workspace_details_card` test at the implementation commit in this directory's history.

| Before | After | Why |
| --- | --- | --- |
| Branch, path and agents had equal emphasis. | Branch uses weight 600; path and agents use quieter type with full foreground in high contrast. | Location can be scanned before supporting context. |
| Task title and state shared a single middle-ellipsized line. | Title wraps to two lines; complete state sits below it. | A long title cannot hide the state. |
| Long labels determined popup natural width. | A native AdwClamp limits preferred content width to 300px. | Branch/path/title ellipsis and wrapping work within a narrow window. |

- `before-{dark,light}.png` and `details-{dark,light}.png` compare the normal fixture.
- `details-narrow-{dark,light}.png` uses long branch, path and task text at a 360px window.
- `details-large-high-contrast.png` and `details-wide-font.png` use Sans 18 and DejaVu Sans 18 with the app's high-contrast class.
- `details-rtl.png` checks logical alignment in RTL.

The native test passed with exact long-path clipboard bytes, original-path copy, complete branch/task tooltips, separate visible state, PR context, action bounds, Changes activation and the same mounted VTE. Before implementation, the separate-title assertion failed on the combined label. The extended stress test then exposed unrestricted popup width and passed after the native clamp.

Grok 4.7 High independently audited the original captures and source. Adopted Details hierarchy and retained the centered shell identity. State appears below the title to avoid horizontal competition. Its separate suggestion to ellipsize approval choices was rejected because permission scope must remain readable without hover. Pane tools were already correct and remain untouched.

The isolated Xvfb run exercises native GTK, system fonts and class-based high contrast. Manual screen-reader order/names, on-screen keyboard, IME, external URI/folder launch and a real desktop high-contrast preference were not run. The full verification report and final reviews are recorded with the PR.
