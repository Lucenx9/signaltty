# Live board review — 2026-10-08

## Behavior and diagnosis
Production App refresh used to force-close and rebuild the board. The initial native regression caught replacement of the dialog. The controller now reconciles task-ID rows and permanent column lists, updating metadata and the current pane destination.

Native regressions additionally caught newer focus being overwritten, loss of focus during removal/migration, and the wrong neighbor after a removal plus another event. Immediate identity restoration prevents GTK's first-row fallback between events; a shared restore point retains the original viewport anchors until allocation. Frame-clock ticks replace a wall-clock delay. New navigation owns its column and horizontal position; other columns keep their anchors.

The narrow geometry regression timed out when a migrating focused card remained clipped. Ranked explanations were stale viewport coordinates, competing native adjustment animation, and missing CSS padding. Instrumentation showed the card's stable content x=424, while the content border box started at x=-18: the outer coordinate was442. Subtracting the actual content border origin fixes the18px gap without hardcoded CSS values. All temporary debug logs were removed.

## Independent review
Grok4.7 through the direct xAI provider identified burst focus loss and stale anchor restoration. Gemini3.8 Flash High on Antigravity identified horizontal reveal, original-column neighbor fallback, allocation timing and stale navigation between events. The tests now cover those cases and current pane activation. Sonnet5.5 was attempted through Claude Code/T3 but hit an API rate limit; that attempt provides no review approval.

Gemini's suggestion that App retains the board through the whole close animation does not match libadwaita1.10: [`sheet_closing_cb`](https://raw.githubusercontent.com/GNOME/libadwaita/1.10.0/src/adw-dialog.c) emits `closed` at the start of sheet closing. App drops the controller and invalidates queued restores then. A closing flag additionally protects standalone board users. Closing with a pending restore is tested.

## Proof and limits
The two isolated GTK tests use production App/controller with fixture task events. They cover dialog/row identity, updated labels/classes, surviving viewport anchors, insertion, focused/unfocused column movement, rapid updates, newer focus/manual horizontal position, original-column fallback, paneNone→new destination activation, empty transitions and pending-close cleanup. Existing async board-dismiss navigation remains in the full gate.

Only X11/Xvfb with isolated D-Bus and explicit `GDK_BACKEND=x11`/Cairo captures counts as native evidence here. An inherited Wayland backend made exploratory timing/capture attempts invalid for isolation; no screenshot from those attempts is retained. The full runner already pins X11. No paid/live provider session, screen-reader, actual-desktop high contrast or IME claim is made. Narrow App scenes retain the existing356px native minimum request warning with350px inside CSD; no header redesign is included.

A final native regression reproduced “pending update overwrote manual horizontal scrolling”. Per-adjustment epochs now let manual horizontal/vertical scrolling after an update win over its deferred restore. Writes made by reconciliation/restoration are excluded from those epochs. Initial native focus animation finishes before the baseline viewport is sampled.

Full gate passed in `target/verification/full-hdvy7agz`:40 steps /28 isolated GTK tests, workspace checks, architecture/warning policy, real-server QA and refresh benchmark. Native [updated board](2026-10-08-board-live/board-live-update.png), [light](2026-10-08-board-live/board-live-light.png) and [dark](2026-10-08-board-live/board-live-dark.png) captures were inspected. The [source hashes](2026-10-08-board-live/verified-sources.json) record the test-only animation-wait update during the run; application code was unchanged. [Verification summary](2026-10-08-board-live/verification-summary.json).

Gemini round3 found no remaining code blockers before the final adjustment-epoch follow-up. The final manual-scroll regression and both native tests pass with that follow-up, and the full gate passes. Direct Grok final round4 remains pending at publication; prior rounds supplied concrete findings and their resolutions above. No approval is claimed for a pending review.

The user requested this PR's merge and then stopping. The initial Needs you reveal follow-up is deferred and is not included here.
