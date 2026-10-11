# Verification

Base `637779bde0e9f41c11142f16ccb32ce72f6a8fec`, stacked on PR100. Final full-run revision and review outcomes will be recorded after acceptance.

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
