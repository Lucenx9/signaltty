# Selectable project sidebar, 2026-10-13

Final source `64ab3107370c606546e88fb0bcec9e59774dc7e9`, base main `553343f`
(including the merged symbolic icon fix #102). Later delivery commits contain
verification records and documentation only.

`scripts/verify.sh full --output /tmp/project-workspace-verification-final3` passed
on a clean checkout: 53 steps, all 41 isolated GTK tests, workspace tests, architecture
and warning policies, real server QA and refresh benchmark. [Summary](summary.json)
records the exact revision, commands, outcomes and tool versions.

| Before | After | Why |
| --- | --- | --- |
| One workspace list | Pinned searchable project selector and All projects | Focus on one repository without switching the active terminal |
| Linked checkouts appear unrelated | Canonical main/subdirectory/worktree membership | Related work stays together |
| Duplicate project names are ambiguous | Full popup paths and secondary trigger context | Recognize the selected repository |
| Creation follows active terminal cwd | Selected project folder is proposed and remains editable | Start work in the chosen project |

Before: [light](before-light.png), [dark](before-dark.png). These are unchanged-base
native worker fixtures. After: [light](project-sidebar-light.png),
[dark](project-sidebar-dark.png), [All projects](project-sidebar-all-dark.png).
The new populated fixture has different sessions; compare control placement and
behavior rather than treating the images as an identical-data pixel comparison.

The native popup is a separate GTK surface, captured directly:
[light](project-picker-light.png), [dark](project-picker-dark.png),
[no match](project-picker-no-match.png), [360px](project-picker-narrow-dark.png),
[large text](project-picker-narrow-large-high-contrast.png) and
[RTL](project-picker-narrow-rtl.png). The separate
[actual native high-contrast run](native-high-contrast.log) passed with
`ADW_DEBUG_HIGH_CONTRAST=1`; [its capture](project-picker-native-high-contrast.png)
also uses Sans 18. No mock UI or image retouching.

[Red excerpts](red-results.txt) precede the relevant fixes: absent metadata/selector,
search label/home-path mismatches, navigation generation and nested task lineage.
The full gate also detected hidden-window selection and direct child auto-expansion
regressions. Final source restores explicit expansion while passive refresh preserves
collapse; all existing and new native regressions pass.

Core tests cover directory identity, aliases, authoritative versus retained catalogs
and legacy scope migration. Real Git tests cover literal linked-worktree paths,
main/subdirectory membership and legacy snapshot backfill after restart. Native
App tests cover searchable choices, All/Enter/Escape/focus, retained row/terminal
identity, nested tasks with excluded ancestors, palette/attention navigation,
creation folder defaults and isolated scope storage. Existing creation validation,
pending-close and retry tests remain green. Scoped creation success is supported by
its metadata/reveal paths; this fixture separately verifies the folder default.

Independent Standards review: 0 documented breaches, one nonblocking judgment
about the inherited synchronous Git enrichment/locked refresh path. Startup backfill
and worktree binding are bounded; redesigning the existing synchronous path is
outside this sidebar change. Independent Gemini 3.8 Flash High Spec/UX review:
0 remaining findings after the search and task-selection fixes. See
[verification record](../../../specs/054-project-workspace-sidebar/verification.md).

Desktop spoken screen-reader order, OSK, IME, folder chooser, desktop notification
clicks and paid provider sessions: not run. Xvfb layout and widget-focus checks do
not certify those paths. Temporary resize/allocation diagnostics remain in full
logs; settled popup captures and every assertion passed.
