# Approval reconciliation evidence, 2026-10-08

The native regression first failed with `the same decision ID becoming read-only must remove stale choices`. After the correction, the same test passes and also verifies restoration, label-only changes, option-ID changes, emitted callback IDs and stable button and VTE identity.

| Before | After | Why |
| --- | --- | --- |
| Same decision ID becoming read-only kept old choice buttons. | Choice buttons disappear and the existing terminal-answer hint remains. | The interface follows the current answer channel. |
| Same-ID option changes retained old labels and callback IDs. | Current labels and IDs replace the old choices. | Displayed choices match the answer the user sends. |
| Prompt-only updates kept buttons and the terminal. | Both still retain their identity. | The correction preserves existing focus and session behavior. |

Native renders: [current light](approval-current-light.png), [current dark](approval-current-dark.png), [read-only light](approval-read-only-light.png), [read-only dark](approval-read-only-dark.png).

`scripts/verify.sh full` passed on the recorded clean implementation commit after rebasing onto main with PR #35. It includes workspace tests, Clippy warning policy, server QA, all 22 isolated GTK tests and refresh benchmark. [Verification record](verification.json).

Grok 4.7 High on the direct provider found the defect. DeepSeek V4.1 Flash High through OpenRouter implemented the production-only correction. Sonnet 5.5 High through Claude independently reviewed the diff and found no blockers. Root reproduced the failure, reviewed the code, strengthened the label-only assertion, fixed the new capture-code Clippy warning and owned final verification and image inspection.

Proof limits: fixtures drive native GTK controls and callbacks through the existing test channel. They do not certify paid provider sessions, screen-reader order or on-screen keyboard input. Concurrent-click handling, resize failures and board dialog refresh are separate issues.
