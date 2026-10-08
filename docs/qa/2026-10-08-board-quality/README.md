# Task Board quality evidence, 2026-10-08

The native 360px board previously clipped trailing columns. The regression failed with `overflowing columns must be scrollable`, alongside an Adwaita warning of 1054px requested with 350px available. Native window decorations explain the 350px content width.

| Before | After | Why |
| --- | --- | --- |
| Trailing columns were unreachable in narrow windows. | Native horizontal scrolling and focus reveal Done. | Existing cards remain reachable without changing the board model. |
| Metadata opacity was 0.65 with a redundant dim class. | A single foreground alpha of 0.8. | Secondary context is more readable. The old opacity rules competed; they did not multiply. |
| High-contrast column headings retained opacity 0.75. | Headings inherit full opacity. | The column names stay readable. |

[Before narrow window](board-before-narrow.png), [light Done column](board-narrow-light-done.png), [dark Done column](board-narrow-dark-done.png), [enlarged text with high-contrast CSS](board-narrow-large-high-contrast-done.png).

The targeted native regression passed all three appearances and checked scrolling, scroll-to-focus, card activation and the pane identifier. `scripts/verify.sh full` passed workspace tests, warning policy, server QA, all 22 isolated GTK tests and refresh benchmark. [Verification record](verification.json) identifies the tested revision plus dirty implementation state. The final documentation and evidence were recorded afterward.

Grok 4.7 High ran through the direct Grok provider. Gemini 3.8 Flash High on Antigravity audited and implemented the two presentation files. DeepSeek V4.1 Flash High on OpenRouter audited independently. Sonnet 5.5 High on Claude reviewed the final diff and found no material blockers after the test sequencing and research wording were corrected. The root inspected the diff and native images and owned verification.

Known proof limits: enlarged high contrast exercises application CSS in the isolated test. Screen-reader order, on-screen keyboard and real-desktop accessibility checks were not run. Long labels still ellipsize and have complete tooltips. Task-event refresh still rebuilds the dialog and resets scroll; the reviewers identified that existing lifecycle defect for separate work. This PR corrects layout containment and typography.
