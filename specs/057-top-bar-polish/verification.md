# Verification

Source `d10dbacdebc28861f982733542b3cffedb40fb18`, integrated base `f3f42f2`. Final full gate passed on a clean checkout with `RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/top-bar-full-pty-20261014`: all 54 steps and all 42 isolated GTK tests, plus refresh benchmark. Serial scheduling preserved every check and timeout. Documentation/proof delivery follows this source revision.

Focused active-workspace and native high-contrast runs passed. Long-title narrow allocations improved from 31px to 126px at Sans11 and from 25px to 126px at Sans18. Active attention retains 73px. Keyboard traversal with a selected workspace is New Tab, Details, Attention; Details preserves the full path. Changes docking adapts toolbar labels to content width. Native focus, RTL mirroring, compact tool actions/toggle synchronization and VTE identity are exercised by existing native tests.

Both independent review axes passed with no actionable findings. Native before/after, RTL, enlarged text and high-contrast captures were inspected by the implementer and reviewer. [Release evidence](../../docs/qa/top-bar-polish-2026-10-14/README.md) includes red/baseline logs, final native logs, full report, screenshots and [reviews](../../docs/qa/top-bar-polish-2026-10-14/reviews.md).

An earlier full run was superseded to integrate main; another was interrupted by SIGTERM before a complete result. The release gate is the completed persistent-terminal run above. Actual compositor dragging, screen-reader speech/order on a real accessibility bus, OSK, IME, folder choosers, notification clicks and paid provider sessions were not run. Xvfb geometry and accessible GTK names do not certify those manual paths.

## Full-width correction

Source 7416ffe on base 0a2cddd passed a clean full gate at `/tmp/full-width-header-full-final-20261014`: 55 steps and 43 isolated GTK tests plus benchmark. A separate 2560px display run of the same binary passed with actual 2390px LTR/RTL windows. Short-name allocation is 58px for 58px ink, compared with 197px baseline allocation. Narrow title remains 120px, active attention 67px. Both review axes passed; the transient 1280px light comparison was excluded in favor of an equivalent stable 890px pair. [Corrective release evidence](../../docs/qa/full-width-header-2026-10-14/README.md) retains logs, source pin, reviews and native renders. Only documentation/proof changed after the gate. Actual compositor fullscreen/dragging, screen-reader speech, OSK and IME were not run.

## Centered identity clarification

Source `c260e947336a2a4f62b9f094c6e6cbd072019d53` on base `bc39d5b` passed a clean 55-step full gate, including 43 isolated GTK tests and refresh benchmark. Dedicated native wide test and native high-contrast run passed. Group midpoint offset is 0px at 1270px and 2390px LTR/RTL; short-name allocation 58px equals ink 58px. Narrow long title 128px, active attention 75px. Independent Spec and Standards reviews have no findings. [Centered correction evidence](../../docs/qa/centered-header-2026-10-14/README.md) retains equivalent stable before/after captures, logs and reviews. Compositor fullscreen/dragging, screen-reader speech, OSK and IME were not run. Only proof/docs follow this source.
