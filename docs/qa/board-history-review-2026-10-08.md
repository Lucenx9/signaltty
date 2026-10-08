# Board history subset review — 2026-10-08

## Scope and diagnosis
The native 25-task fixture failed with “truncated history needs explicit feedback”, after asserting the total heading, the cap of 20 and newest-first order. Ranked hypotheses were missing notice, hidden notice, incorrect total. Total/order/card count were correct; no notice widget existed. The conditional native wrapping label fixes that root cause without changing history retention or task classification.

| Before | After | Why |
|---|---|---|
| Done · 25 with only 20 cards | Total 25 plus “Showing latest 20 of 25” | Explains the existing cap without suggesting missing/deleted records |
| Short complete list | No additional notice | Avoid redundant metadata |

## Native verification
`task_board_explains_truncated_done_history` uses actual GTK board controls with 25 versus20 tasks. It checks the first/last visible card and cap, conditional feedback and mapped notice bounds. Light and dark/Sans18 renders show the full wrapping notice inside the Done column. Full gate and independent review are recorded after completion.

The change follows apple/emil feedback and restrained native hierarchy, and t3code collapse-and-count. No CSS token, motion, dependency, state or IPC change. Pointer tooltips and ellipsized task labels remain; screen-reader, actual desktop high contrast and paid provider sessions are not certified by Xvfb. Source snapshots are fixtures, not production task history.

Sonnet5.5 read-only review found no evidence blockers in the diff or captures. Grok4.7 direct xAI scope review is pending; all review findings and full verification are resolved before merge.

Full gate passed in `target/verification/full-4wc6lgij`: 37 steps / 25 isolated GTK tests, workspace checks, real-server QA and refresh benchmark. Durable native captures and source hashes are in [2026-10-08-board-history](2026-10-08-board-history/). No code changed after this run.
