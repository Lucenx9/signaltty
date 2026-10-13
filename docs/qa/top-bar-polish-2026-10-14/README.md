# Top bar native evidence, 2026-10-14

Source revision `d10dbacdebc28861f982733542b3cffedb40fb18`, integrated base `f3f42f2`. The baseline uses `9d1913a` with capture-only measurement instrumentation. The repository feature is [spec057](../../../specs/057-top-bar-polish/spec.md).

| Native fixture | Before | After |
|---|---:|---:|
| Long title, actual 350px window, Sans11 | 31px | 126px |
| Long title, actual 350px window, Sans18 | 25px | 126px |
| Enlarged text with active attention | not measured | 73px |
| Desktop with Changes docked | not measured | 144px |

These are allocated title widths in this fixture, not universal font metrics. Native HeaderBar groups lead creation/navigation, give identity the title lane and trail attention/views/menu. A concealed counter previously reserved 35px; direct button hiding releases it. Content-local thresholds avoid squeezing labels when Changes is docked. Compact labeled sidebar tools retain existing actions and toggle bindings.

## Captures

Before/after desktop light/dark and narrow Sans18 use the same header fixture. `narrow-attention-dark.png` retains Approval and the attention counter. `docked-changes-light.png` and `docked-changes-rtl-dark.png` show actual content-width adaptation. `native-high-contrast-large.png` comes from a process with `ADW_DEBUG_HIGH_CONTRAST=1`, asserted through StyleManager. `compact-tools-large.png` is the footer with enlarged text and the application's high-contrast CSS class; this is distinct from the asserted native high-contrast run.

`baseline.log`, `red-group-order.log` and `red-hidden-counter.log` retain baseline and staged failing regression checks. `native-header.log` and `native-high-contrast.log` retain passing active-Details/attention focus order, title allocations, complete Details metadata, docked/RTL geometry and retained VTE identity. `reviews.md` records the two independent review axes.

## Proof limits

Captured through isolated Xvfb/D-Bus with production resources and CSS. Accessible control names and GTK focus were exercised; screen-reader speech/order on a real accessibility bus, OSK, IME, actual compositor dragging, folder choosers, notification clicks and paid provider sessions were not run. The drag region's native allocation is checked, not its compositor gesture. Native snapshots include transient measurement/allocation warnings during resize; no passing assertion or screenshot substitutes for an unperformed manual check.

An earlier full run was superseded when main advanced. A subsequent run was interrupted by SIGTERM before completion. Only the completed final gate in `verification.json` is release evidence. Serial Rust test scheduling preserves all checks and their original timeouts on this contended host.

## Completed gate

`RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/top-bar-full-pty-20261014` passed all 54 steps, including all 42 native GTK tests and the refresh benchmark, on the clean source revision above. `verification.json` retains step commands, exit codes, durations, versions and source identity. Selected complete logs are retained here; remaining runner logs are in the local evidence directory named in that command. `workspace-dark.png` shows the populated native fixture with seeded terminal output; actual test outcomes come from logs and the verification report.
