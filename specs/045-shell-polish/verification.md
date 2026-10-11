# Verification — 2026-10-11

## Scope

Native GTK/libadwaita visual polish on base `fc1a517`. No server, IPC, PTY,
navigation or persistence changes. Three design alternatives and primary-source
references are recorded in research.md and exploration.html.

## Evidence and limits

The new isolated display test renders populated scenes and both empty states in
light/dark, desktop/narrow and Sans 18 with the application high-contrast CSS
class. It checks primary action names, platform-formatted shortcut text and
button/shortcut bounds, and activates New Workspace to its native dialog.
The populated terminal output is deterministic fixture text, not test results.

Native screenshot inspection found the original empty-tab description clipping
at Sans 18/360px even after the compact layout. Final copy is “Start a shell.”;
visual inspection is required in addition to button/shortcut geometry assertions.

Xvfb proves isolated widget behavior, not real compositor or accessibility
certification. Real screen reader, desktop high-contrast theme, IME, OSK, folder
chooser and live paid agent sessions were not exercised. Existing native titlebar
controls can be cramped at narrow width with enlarged desktop fonts; this pass
changes the page content, not native decoration behavior.

## Review

Independent native reviewer found no blocking findings in the diff and populated
light/dark scenes. Gemini 3.8 Flash High via Antigravity also found no blocking
static findings; its first round requested inspection of final empty-tab renders.
The focus-outline observation was a product preference, not a defect.

Gemini's second Antigravity review completed against the final native renders and
passed gate: no blocking findings. It closed the pending P2 visual follow-up;
“Start a shell.” eliminates clipping in the large light/dark scenes.
Review round IDs: `signaltty-045-gemini-review-round-1` and
`signaltty-045-gemini-review-round-2` (provider `antigravity`, model
`gemini-3.8-flash-high`). The independent native review also accepted the final
large-font scenes without findings.

## Final gate

`scripts/verify.sh doctor` passed (`doctor-hx97dm8c`). Final
`scripts/verify.sh full` passed: `target/verification/full-numuux3w`, 46 steps,
including 34 isolated GTK tests, workspace tests, server QA, Clippy/warning
policy, architecture, build, formatting and refresh benchmark. The first full
run also passed; the final run supersedes it after the copy correction.
Summary is retained in evidence/verification-summary.json.

Parent and independent native reviewer inspected final populated and empty-state
captures; “Start a shell.” is fully visible in large light/dark scenes. All
selected captures are in evidence/. The no-tabs desktop scene intentionally
exposes the existing checked Changes panel; its unavailable-data notice comes
from the minimal mock IPC actor, not a live-server failure.
