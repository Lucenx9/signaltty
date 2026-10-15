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

## CI correction, 2026-10-15

The first proposed-merge run 38752638254 failed gtk-15 on 91a4e17. Its real wide-font screenshot is retained as ci-wide-font-before.png: at 360px and large text the trailing shortcut squeezed New Workspace. The same assertion was reproduced locally using DejaVu Sans 18, so this was a genuine allocation defect, not a retryable environment failure.

| Before | After | Why |
| --- | --- | --- |
| Shortcut competes horizontally with title under 480sp | Native command subtitle shows shortcut beneath title | Retain both complete labels with wider font metrics |
| Only generic Sans 18 coverage | Additional DejaVu Sans 18 stress case | Exercise the metrics that exposed the CI failure |
| No resize placement check | Narrow→wide→narrow assertions | Prove trailing hint restores without duplicate subtitle |

Exact narrow test passed after native breakpoint setters. The original no-ellipsis assertion remains. Test now checks the mapped shortcut because its trailing copy is intentionally hidden in compact mode. wide-font-after.png and shortcut-restored-wide.png show actual native results; current narrow/RTL images reflect the corrected presentation. The change does not touch search, target mapping, activation or workspace path subtitles.

Independent Spec and Standards delta reviews each report zero findings on 91a4e175…5142b4fa. Both reviewed the actual wide-font/native restoration captures and the unchanged no-truncation assertion. Finalization after 5142b4fa adds only evidence and task readiness.

Fresh full execution report for this corrected source will be /tmp/palette-recognition-ci-fix-full-20261015, with actual verified revision/result in the updated PR body. Earlier full success belongs to 91a4e17 and is not proof of corrected source. This paragraph freezes before the new run and does not claim its outcome.
