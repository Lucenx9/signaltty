# Worktrees toolbar verification, 2026-10-15

Base `63c5a5f55bfefd5dc62fc211b7d69a0efbb61fdc`, independently reviewed source `0ef7a8211f1cdbe290d6c43f98074f20d1ebc6cf`. Actual native GTK captures from the existing navigation/dialog integration fixture, with production resources and isolated actor boundary.

| Before | After | Why |
| --- | --- | --- |
| Ellipsized Create Worktree… in desktop header | Fully readable New with plus | Window title supplies the checkout context |
| Filled accent creation control | Native flat header action | Reduce toolbar weight and preserve hover/focus feedback |
| Icon-only action under narrow breakpoint | Same plus with complete accessible name and tooltip | Preserve existing adaptation and keyboard reachability |

Existing integration test `app::tests::navigation_palette_fast_enter_and_git_dialogs_use_native_controls` passed on base, then failed after the new expected-label assertion. It passed after the presentation change and waiting for breakpoint label restoration on resize. The first green attempt exposed a test synchronization error: the control was already mapped before the restored label was allocated. Waiting for the actual label fixed that assertion. The final test cleanup removes repeated style assertions after widening; full verification runs that exact final source.

Inspect dark/light desktop and narrow Sans18 captures. The test verifies untruncated New labels, flat style, header placement, target bounds, row-action keyboard scrolling, creation/open/removal confirmations and error handling. Accessible name/tooltip and native plus/breakpoint/handlers are unchanged in source. This is not real-desktop screen-reader or on-screen keyboard certification.

Independent code-review Spec and Standards axes each report zero findings on the pinned range. Spec review checked FR001–003 and light/dark/narrow images; Standards review checked repository rules and Fowler heuristics. Full verification is a separate gate. Source-finalization delta adds this evidence and task readiness only.

Final full report is external at `/tmp/worktrees-toolbar-full-20261015`; actual result/revision and CI artifact are recorded in the PR body after execution. No advance full-success claim here. Rollback changes only two presentation properties, with no migration or operation semantics.
