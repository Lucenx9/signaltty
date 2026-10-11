# Verification — 2026-10-11

Application revision: `0ebff6583a525bc6fccb7f90654da396b62cd846` (clean).
Gate: `scripts/verify.sh full --output /tmp/pro-ui-verification-final` — passed.
[Complete run record](../../docs/qa/workspace-refinement-2026-10-11/summary.json).
Workspace tests, zero-new-warning policy, real server QA, all 35 isolated GTK tests
and refresh benchmark passed. Direct `cargo build --workspace` also passed.

The new headline regression failed first on generated Working… then passed;
explicit messages, attention and timed labels stay visible. New GTK regression
checks status visibility, metadata choice/hiding, tooltips and retained widgets.
The shell fixture asserts the high-contrast class after appearance switches and
checks minimum 200px rail bounds with normal/enlarged desktop fonts.

## Native visual review

[Before sidebar](../../docs/qa/workspace-refinement-2026-10-11/before-sidebar.png)
and [after sidebar](../../docs/qa/workspace-refinement-2026-10-11/after-sidebar.png)
are crops of owned, live GTK windows with real PTYs in isolated Xvfb/D-Bus sessions.
Branch names and project mark colors differ between temporary sessions.
[Final light](../../docs/qa/workspace-refinement-2026-10-11/light.png) and
[final dark](../../docs/qa/workspace-refinement-2026-10-11/dark.png) retain readable
activity, symbolic metadata, trailing status and distinct selection.

[Enlarged high-contrast light](../../docs/qa/workspace-refinement-2026-10-11/rail-large-hc-light.png)
and [dark](../../docs/qa/workspace-refinement-2026-10-11/rail-large-hc-dark.png)
retain urgent Approval and directory context inside the 200px rail. Long names
ellipsize, full tooltips remain, and selection boundaries are explicit. The
existing native suite covers narrow empty states, keyboard navigation and stable
terminals. High contrast here is the application class on an isolated display.

Independent Standards and Spec reviewers approved final source and visual evidence.
Their two initial findings were corrected: bundle the directory icon, and add the
high-contrast class after the appearance switch. Initial captures are superseded.
An earlier full run was deliberately interrupted for those fixes; only the final
passed record above is delivery evidence.

Not run: real-desktop screen reader, on-screen keyboard, IME, notification clicks,
folder chooser and paid provider sessions. This pass adds no new interactions,
external APIs, fonts, providers or toolkit dependencies.
