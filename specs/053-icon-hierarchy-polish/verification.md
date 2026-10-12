# Verification

Base `637779bde0e9f41c11142f16ccb32ce72f6a8fec`, stacked on PR100. Clean code revision `e573f551134b96d6225df87f5e62c9b773c434c9` passed full verification. Durable [QA evidence](../../docs/qa/icon-hierarchy-polish-2026-10-12/README.md).

## Visual decisions

| Before | After | Why |
| --- | --- | --- |
| Changes panel resembles pane split | Shared diff-document plus/minus | Distinguish reviewing code from arranging terminals |
| Generic Board grid | Three-column board | Match the destination's structure |
| New Tab among trailing tools | Labeled creation after workspace context, compact tab-strip plus | Put creation beside what it affects |
| Board/Changes separate header packing | One trailing view group, spacing before menu | Communicate shared scope without another background |
| Pane Close adjacent to split pair | Logical gap, same targets | Separate closure from layout changes |
| Worktree window actions occupy body row | Create/Refresh in header, Open/Remove on checkout | Match window and row scopes |
| Large narrow Worktrees cuts off actions | Native compact icon controls, full names/tooltips, scrollable rows | Keep actions reachable at 360px and Sans18 |
| Metadata downsampled to14px, mixed details symbols | Owned16px decorative symbols, flat Copy button | Consistent alignment and quieter hierarchy |

## Test-first evidence

Unchanged parent native worker, Details and navigation fixtures passed, captured under `/tmp/icon-hierarchy-baseline`. Red checks preceded production changes: owned Changes resource missing; New Tab desktop label absent; pane Close lacked separation; Worktree Create outside header. Raw logs `/tmp/icon-red-*.log`. The narrow Worktree enlarged-text bounds check also failed before the responsive fix.

## Proof limits

Native test renders use production CSS/widgets and isolated IPC seams; full also runs real server/PTTY QA and all ignored GTK tests in separate displays. High contrast will be checked using native Adwaita debug mode as well as application contrast classes. Automated keyboard focus/order and decorative roles do not certify spoken screen-reader order. Desktop screen reader, OSK, IME, chooser, notification clicks and paid provider sessions are not run. Subjective aesthetic judgment remains reviewable through retained native captures.

## Final acceptance

`scripts/verify.sh full --output /tmp/icon-hierarchy-verification-accepted`: passed 51 steps, 39 GTK tests on clean e573f55. All workspace tests, architecture/warning policy, real server QA and benchmark passed. Native high-contrast icon and Worktree fixtures passed at the same revision. Light/dark, enlarged/narrow, RTL, pane focus/targets, retained VTE and feature052 safe closure/retry tests passed. Selected actual before/after renders and test-first red excerpts are committed with the runner summary.

Standards axis: 0 documented violations and 0 actionable smells. Spec/UX axis: 0 remaining findings. A mistaken missing Refresh/Close observation was withdrawn after the reviewer inspected current exact-head normal/high-contrast images. No code change was justified by that observation. The earlier interrupted full run is excluded from completion evidence. Subsequent documentation-only commits do not alter the tested code.

## Base update during delivery

PR100 merged while this task ran. Its final source adds a pending-operation parent-window close guard and15 regression lines. Rebase onto main745f72f was conflict-free and imports that fix. Original 51/39 evidence remains historical; a clean final full run against the rebased revision will replace the delivery summary. Review diff against main contains only this polish.

Final delivery gate: clean a3f022b against main745f72f passed 51 steps/39 GTK via `/tmp/icon-hierarchy-main-verification`. Refreshed QA captures and summary are committed. Both review axes reconfirmed 0 findings after rebase. The parent pending-close fix is preserved; later commits contain evidence/documentation only.

Delivered [PR101](https://github.com/Lucenx9/signaltty/pull/101) against main and registered it with T3. Final delivery commits are documentation only.
