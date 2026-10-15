# Task Board density verification, 2026-10-15

Base320bf9317a70071c6d78732c152760abb4a1924c; implementation reviewed at9cf4a6cdf34db86fc1fa237b80e87a403b1c8028. Captures are real native GTK renders, not mockups. Before is the prior full-run Board render; its layout matches the base Board code. After captures come from the exact existing display tests below.

| Before | After | Why |
| --- | --- | --- |
| 600px requested content height | 440px, stable during updates | Sparse boards cover less terminal canvas |
| Subtitle competes with worker badge | Full-width context wrapping up to three lines, state beneath | Branch and PR facts get usable width |
| Filled Completed badges in several PR columns | Explicit success text without filled background | Keep worker state clear with less visual noise |

`app::tests::task_board_shows_columns_and_navigates_to_pane`: red on base because requested height exceeded460; green after implementation. Initial test compile typo wrap→wraps was corrected before recording behavioral red. `task_board_fits_narrow_windows_and_reveals_last_column` and `open_task_board_preserves_identity_scroll_and_focus_on_updates`: green in separate Xvfb/D-Bus processes. Set SIGNALTTY_UI_EVIDENCE to capture; see feature067 quickstart. Exact assertions cover height, full-width vertical context/state siblings and wrapping; existing scenarios cover activation, stale PR reevaluation, narrow navigation and live row/focus/scroll stability.

Independent code-review axes both report zero findings on320bf93…9cf4a6c. Spec reviewer checked FR001–004 and desktop/narrow captures; Standards reviewer checked AGENTS, constitution, docs06/14/16/17/18/19 and Fowler heuristics. Both confirm native state semantics and preserved reconciliation. Source-finalization delta adds only verification docs/evidence and task readiness.

Final shared full run is recorded outside source at /tmp/board-density-full-20261015; actual result, verified revision and CI artifact belong to the PR body. This document freezes before that run, so it makes no advance claim of full success. Class-based high-contrast/enlarged-text Xvfb is not certification of real desktop theme, screen-reader, on-screen keyboard, IME or paid provider sessions. No new manual certification claimed. Rollback is GUI-only with no migration.
