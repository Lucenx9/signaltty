# Icon hierarchy polish, 2026-10-12

Verified code `e573f551134b96d6225df87f5e62c9b773c434c9`, base `637779bde0e9f41c11142f16ccb32ce72f6a8fec` from PR100. Later delivery commits contain documentation/evidence only.

`scripts/verify.sh full --output /tmp/icon-hierarchy-verification-accepted` passed on a clean checkout: 51 steps, including all39 isolated GTK tests, warning/architecture policy, workspace tests, real server QA and refresh benchmark. [Summary](summary.json) records exact commands, outcomes and tool versions.

The test-first [red excerpts](red-results.txt) precede production changes. Baseline captures come from unchanged parent native worker/Details/navigation fixtures; after captures are production GTK widgets/CSS from the clean full run. No mocked UI or retouched images.

| Before | After | Why |
| --- | --- | --- |
| Changes resembles split controls | Diff document plus/minus shared across main/compact/details | Reviewing code has a distinct role |
| Generic Board grid | Three columns | Match the destination |
| Unlabeled creation among trailing tools | New Tab after workspace context, desktop label | Creation sits beside its target |
| Split and Close adjacent | Same paired splits with a gap before Close | Separate layout and closure |
| Worktree actions in body row | Create/Refresh in header; Open/Remove on checkout | Window and row scopes are clear |
| Large narrow controls clipped | Native compact icons, complete names/tooltips, height for wrapping, focus scroll | Preserve reachable actions |
|14px metadata and mixed details symbols | Owned16px decorative icons and restrained Copy | Consistent alignment/hierarchy |

Before: [app](signal-dark-before.png), [Worktrees](worktrees-dark-before.png). After: [app dark](signal-dark.png), [app light](signal-light.png), [Worktrees dark](worktrees-dark.png), [Worktrees light](worktrees-light.png). [Narrow enlarged row focus](worktrees-narrow-large-row-focus.png) and [native high contrast](worktrees-narrow-large-native-high-contrast.png) include Create/Refresh/Close and checkout actions.

Additional native `ADW_DEBUG_HIGH_CONTRAST=1` passes: icon sheet, Worktree navigation, toolbar focus/grouping and Details copy/handoff. The last two were run on b7ff342; their source did not change in e573f55. Icon/Worktree checks passed again on e573f55. Default full-run captures cover light/dark, 360px, enlarged text and logical RTL. Tests prove header x/y bounds and mapping; focusing row operations scrolls them into the window and widening restores labels.

Independent Standards review: 0 breaches, 0 actionable smells. Independent Spec/UX review: 0 remaining findings. A provisional missing-header-controls image objection was withdrawn after reopening exact-head captures; the buttons are visible and their mapped/vertical bounds pass. Review does not replace the full gate.

Manual desktop screen reader/spoken order, OSK, IME, folder chooser, notification clicks and paid-provider sessions: not run. Isolated GTK keyboard/focus/accessibility-role checks are not desktop assistive-technology certification. Native snapshot/temporary overflow diagnostics during other fixture resize transitions are retained in ephemeral full logs; settled target captures and all assertions passed.
