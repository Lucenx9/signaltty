# Palette recognition verification, 2026-10-15

Base `5c51c688c30e4d2328a5c9a25388b87b21f3cea2`, independently reviewed source `4e04aa52971f3f7e853344ab013a54d2fe6604f9`. Captures are actual GTK renders. Before comes from the prior full runner and matches this base palette code. After desktop captures use production resources in the App fixture; narrow scenes register the same app-owned icon resources explicitly.

| Before | After | Why |
| --- | --- | --- |
| Workspace names without their identity marks | Same sidebar monogram and ID-derived tint | Recognize the same workspace across surfaces |
| Text-only command results | Neutral 16px action/context glyph in a 20px leading lane | Support scanning while labels remain authoritative |
| Host-resource setup absent in standalone palette scenes | Explicitly register bundled icons as production does | Capture real glyphs rather than missing-image fallbacks |
| LTR-only prefix proof | Native RTL geometry and screenshot | Verify logical placement without custom mirroring |

`navigation_palette_fast_enter_and_git_dialogs_use_native_controls` demonstrated behavioral red for the missing workspace prefix, then passed. `palette_empty_results_and_shortcuts_fit_narrow_appearances`, `palette_keyboard_selection_stays_visible_while_search_keeps_focus` and `owned_icon_roles_resolve_and_render_without_external_themes` passed individually in fresh Xvfb/D-Bus sessions. The last test resolves every registry icon with external search paths removed. Existing activation/target mapping, filtered groups, no-match feedback, keyboard selection visibility, shortcut fit and workspace identity assertions pass. Prefixes expose presentation roles; existing labels retain their accessible identity.

The first RTL fixture only changed the parent direction, leaving the separate dialog root LTR. Setting the toolkit default direction before construction made it a valid RTL test. Inspecting narrow renders also found missing resource paths in standalone scenes; those now mirror production registration. These were fixture corrections, not production RTL/icon-loading workarounds. Current captures contain real bundled glyphs. High contrast is a window-class/enlarged-font scene, not a certified desktop theme.

Independent code-review Spec and Standards axes each report zero findings on the pinned range. Spec reviewed FR001–004 and desktop/narrow/RTL captures; Standards checked repository rules and Fowler heuristics. Finalization delta adds only this evidence and task readiness.

Final full report is external at `/tmp/palette-recognition-full-20261015`; actual result/revision and GitHub CI artifact belong to the PR body after execution. This document freezes before that run and makes no advance success claim. Not run: real-desktop screen reader, on-screen keyboard, IME or paid provider sessions. Native roles/renders do not certify assistive technology operation. No model, persistence, IPC or dependency change; rollback is GUI-only.
