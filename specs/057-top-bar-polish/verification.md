# Verification

Source `d10dbacdebc28861f982733542b3cffedb40fb18`, integrated base `f3f42f2`. Final full gate passed on a clean checkout with `RUST_TEST_THREADS=1 scripts/verify.sh full --output /tmp/top-bar-full-pty-20261014`: all 54 steps and all 42 isolated GTK tests, plus refresh benchmark. Serial scheduling preserved every check and timeout. Documentation/proof delivery follows this source revision.

Focused active-workspace and native high-contrast runs passed. Long-title narrow allocations improved from 31px to 126px at Sans11 and from 25px to 126px at Sans18. Active attention retains 73px. Keyboard traversal with a selected workspace is New Tab, Details, Attention; Details preserves the full path. Changes docking adapts toolbar labels to content width. Native focus, RTL mirroring, compact tool actions/toggle synchronization and VTE identity are exercised by existing native tests.

Both independent review axes passed with no actionable findings. Native before/after, RTL, enlarged text and high-contrast captures were inspected by the implementer and reviewer. [Release evidence](../../docs/qa/top-bar-polish-2026-10-14/README.md) includes red/baseline logs, final native logs, full report, screenshots and [reviews](../../docs/qa/top-bar-polish-2026-10-14/reviews.md).

An earlier full run was superseded to integrate main; another was interrupted by SIGTERM before a complete result. The release gate is the completed persistent-terminal run above. Actual compositor dragging, screen-reader speech/order on a real accessibility bus, OSK, IME, folder choosers, notification clicks and paid provider sessions were not run. Xvfb geometry and accessible GTK names do not certify those manual paths.
