# Compact workspace hierarchy, 2026-10-13

Source base `d957d93`, implementation `f427197bb6e566521684cb145fca6fa50306b56d`.

The native GTK fixture contains ten workspaces: seven ordinary unselected rows, plus selected, approval and unread rows. At the same window/rail/font configuration the seven ordinary rows occupy 601px before and 435px after, a reduction of 166px, 27.6%. This is fixture geometry, not a general user-productivity benchmark. The baseline used the parent production source with the same populated capture fixture added locally; the after captures use the implementation source. Every screenshot is a native GTK render, not a browser mock.

| Before | After | Why |
| --- | --- | --- |
| Every ordinary row shows its last message | Useful preview follows selection or attention | More names and states fit in the rail |
| State always consumes the activity line | Name and state share space when available; native wrapping protects long titles | Compact ordinary rows and usable enlarged text |
| Header combines branch and full path | Short branch/agent/directory context, full identity in Details and accessible/tooltip text | Header actions have more room |
| Narrow header shares scarce space with secondary context | Secondary context hides in the overlay layout | Workspace identity has priority |

## Captures

- `before-light.png`, `before-dark.png`, `after-light.png`, `after-dark.png`: identical populated fixture.
- `sidebar-large-native-high-contrast.png`: 200px rail, Sans 18 and native `ADW_DEBUG_HIGH_CONTRAST=1`, with StyleManager's native high-contrast state asserted.
- `approval-task-large-dark.png`: approval and worker chip remain separate and readable under enlarged text.
- `header-light.png`, `header-narrow-large-dark.png`: desktop short context and narrow enlarged identity with complete details metadata retained.
- `sidebar-rtl-dark.png`: global native GTK RTL direction asserted; selection and attention still identifiable.

Reproduction: set `SIGNALTTY_UI_EVIDENCE` to a fresh directory and run the exact isolated tests listed in specs/055-compact-workspace-hierarchy/quickstart.md. Each run uses a new Xvfb/D-Bus session and the application CSS/resources.

## Review and proof limits

Independent Standards and Spec reviews found no blocking issues. The Spec review requested clearer fixture wording; SC-001 now specifies the seven ordinary rows in the mixed ten-workspace fixture. Optional refresh-method naming cleanup deferred because the established methods already update presentation as well as time.

The first full run failed on `semantic_hooks_override_grok_progress_and_screen_evidence`, timing out while waiting for Working. The exact test passed in isolation on unchanged source. `first-full-summary.json` and `screen-detection-retry.log` retain both outcomes; final full gate PASS on clean `4577ef2` (54 steps, all 42 GTK tests) is recorded separately in `summary.json`. No server, agent or testkit source changed. This observation does not establish a root cause for the timeout. The runner additionally exercises the existing real-server QA and retained terminal/layout contracts. A screenshot is not a passing interaction check; the separate red logs demonstrate changed behavior before implementation.

Not run: real desktop screen-reader speech, on-screen keyboard, IME, folder chooser, notification clicks and paid providers. Native high-contrast/large-text renders do not certify those workflows. The narrow long title remains ellipsized; full name/path are available through the details button and existing Details card.

The second parallel full run also failed in screen_detection, with eight lifecycle-wait timeouts. The host has four CPUs and contemporaneous load averaged about 9.6, with other independent compilation/test jobs active. Final verification uses `RUST_TEST_THREADS=1` to serialize test cases, without skipping checks, changing deadlines or removing the application concurrency exercised within each case. These observations support reducing test-runner contention but do not establish a code-level root cause.
