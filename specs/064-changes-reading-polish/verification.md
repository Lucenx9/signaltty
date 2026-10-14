# Verification procedure and focused results

Baseline 2d5b4cc. Focused native tests were exercised before implementation and after the layout/reader changes:

- Existing scope test fails red when requiring a mapped Changes title.
- Existing reader test fails red when requiring visible hunk-to-code separation.
- Both modified native tests pass after implementation; copy text, literal values, Back, stable selection/list identity, stale response handling and scope requests remain covered.
- Both pass separately with `ADW_DEBUG_HIGH_CONTRAST=1`, explicitly asserting native high contrast and full foreground for number prefixes.
- Actual App-seam navigation test passes with palette/worktree/Changes workflows and retained terminal widgets.

Baseline and final native renders inspected in light/dark/narrow/enlarged text/RTL/high contrast. Changes labels fit the actual 350px client window, long paths stay inside its bounds, long code scrolls inside the reader. Selected durable captures and focused logs are retained in docs/qa before final source freeze.

Final execution: `RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/changes-reading-full-20261014`, then `scripts/agent.sh check --phase delivery --feature 064-changes-reading-polish --evidence /tmp/changes-reading-full-20261014`. The report owns exact content/index provenance and actual final result. PR body records the verified revision and result after completion; source is not changed after success. Handoff output remains outside the checkout.

Actual compositor behavior, screen-reader speech, OSK, IME, folder choosers and paid-provider sessions are not exercised by these Xvfb fixtures. No manual desktop certification is claimed.
