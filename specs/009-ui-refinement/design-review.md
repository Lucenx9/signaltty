# Design comparison and synthesis

Two independent candidates were produced after tracing GTK ownership and native
preference behavior. Both retained domain state, asynchronous IPC and VTEs.

| Criterion (0–5) | A: existing rows and wrapped choices | B: grouped activity and stacked choices |
|---|---:|---:|
| Narrow approval and attention clarity | 5 | 5 |
| Keyboard/accessibility immediacy | 5 | 5 |
| Native visual hierarchy | 5 | 4 |
| Motion/runtime preferences | 5 | 5 |
| Interface depth and simplicity | 4 | 4 |
| Verification at existing seams | 5 | 5 |
| Independent cross-judge total | **29** | **28** |

A is the base. Its wrapping options preserve terminal density and the current
three-line row keeps the full activity line. A native wrapping container avoids
duplicated responsive controls. Both candidates agreed on separating urgency
from close, immediate focus/control reveal and settings-driven motion handling.

B contributes keeping the small preference policy inside App rather than a
new forwarding module, plus native keyboard/press-state checks. Always-stacked
choices remain an alternative if actual minimum-width tests invalidate wrapping;
they are not combined into a second conditional tree.

No new service, backend contract, command palette, global shortcut mode or glass
material is introduced. Pending-answer state is optional and must follow a
concrete regression for duplicate submission/focus loss before inclusion.

The before screenshot already shows the approval competing with choices and
forcing uneven split allocation. Native allocation, option text and activation
checks decide whether the proposed shape is viable before cosmetic acceptance.

## Standards review

Pass. The implementation follows native GTK ownership, scoped accessibility
styling and persistent terminal reconciliation. No remaining documented violation
or material structural concern was found.

## Spec review

Pass. FR-001–FR-008 are covered by source changes and recorded native evidence.
The 320px result applies to the tested long-question/choice fixture; no universal
claim is made about arbitrary amounts of approval content.

## Motion review

Approve after correction. The first reduced-motion override lost to active-state
selector specificity. Final active-state overrides remove the rendered scale,
proved by a regression that failed with the old CSS and passes with the fix.
Pointer feedback remains 120ms at scale 0.97, keyboard focus is immediate, and
native tab-view interaction remains toolkit-owned.
