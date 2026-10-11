# Verification

Full gate at clean `b15d7f7754b9cea2fe67575ababb88f48af5b78b` passed all 47 steps, including 35 isolated GTK tests, workspace tests, server QA, architecture/warning policy and refresh benchmark. Command: `scripts/verify.sh full --output /tmp/restrained-workspace-shell-verification-final`.

Native light/dark shell, tabs, owned menu, Changes, terminal search and primary/read-only approvals reviewed. Existing narrow and enlarged-text shell tests passed. Additional populated high-contrast shell probe passed at the same revision. Selected durable evidence and reproduction command: [QA record](../../docs/qa/restrained-workspace-shell-2026-10-12/README.md).

| Before | After | Why |
| --- | --- | --- |
| Round window icons and mixed control geometry | Ghost window controls and 6px owned controls | Match dialogs while retaining native targets |
| Raised panes and approval cards | Flat inset borders and existing urgency rings | Keep terminal output primary and approvals visible |
| Capsule task/status chips | Compact 4px badges with existing semantic colors | Reduce repeated pill decoration |
| Filled Changes scope and tabs | Neutral selection with bottom markers | Selection stays clear with lighter hierarchy |
| Search fixture missing production CSS | Explicit CSS-backed captures | Visual evidence now represents the app styling |

Standards axis: zero remaining source or visual findings. Spec axis: zero remaining source or visual findings. Both inspected regenerated search and primary approval captures and closed pending visual acceptance. Menu and confirmation additions preserve actions, default Cancel and dismissal. No terminal/state logic changed.

The first shell gate was intentionally stopped after the context menu/confirmation additions; it is not passing proof. Final gate ran on the complete clean revision. Subsequent commits contain evidence/docs only.

Limits: desktop screen reader, IME, on-screen keyboard, native chooser, notification clicks and paid providers not run. Approval shots use a standalone pane fixture; shell shots use the app's production icon context. No desktop accessibility certification is claimed.

## Codex contrast follow-up

[Contrast proof](../../docs/qa/destructive-dialog-contrast-2026-10-12/README.md) records the explicitly transparent destructive response background and new native confirmation captures. Full gate at clean `18ff1b5` passed 47 steps, including 35 GTK tests. Both follow-up review axes found zero remaining issues. The initial server stream test failure passed in isolation and in the complete fresh rerun without server edits.
