# Verification

## Test-first evidence

Before production edits, isolated GTK regressions failed for the intended behavior:

| Check | Observed failure before | Passing behavior after |
| --- | --- | --- |
| terminal_closures_confirm_live_targets_and_preserve_failed_tabs | `close must wait for confirmation` | Cancel sends no IPC, original target survives navigation, failed tab retains widgets; exited/restored/empty targets close directly |
| dialog_validation_explains_invalid_folder | Missing `Choose an existing project folder.` label | Missing folder explains invalid state and pointer/Enter cannot submit |
| visible_workspace_tools_open_palette_and_board_without_main_menu | `compact sidebar must expose labeled tools` | Labeled compact tools activate existing views/state and hide again at wide widths |
| dialog_confirm_emits_validated_request | `the form must remain open until creation succeeds` | Mounted pending draft, duplicate guard, failure Retry, success dismissal |

The production App creation regression also proves invalid name/command handling, no misleading cancellation while pending, retained choices after create failure, partial-create command correction with stable workspace id, and actual VTE focus after success.

## Delivery gate

Clean `61f3d9849134044b304e1ef2b6b1a93394443b35` passed 50 steps/38 GTK cases via `scripts/verify.sh full --output /tmp/reliable-workspace-interactions-verification-final`. No errors. Includes workspace tests, formatting, reviewed warning policy, architecture, server QA and refresh benchmark. An earlier full b161351 run passed; final clean run covers the QA adapter follow-up. Subsequent commits contain delivery documentation and selected evidence only.

Selected native captures and reproduction instructions: [QA evidence](../../docs/qa/reliable-workspace-interactions-2026-10-12/README.md). Three supplemental GTK cases at identical source passed with an isolated reduced-motion preference to capture settled dialog states. All selected images were inspected. Initial clipped-feedback scene was corrected with native content-size tracking/clamping. No terminal renderer or IPC changes.

## Standards

Independent Standards review: 0 documented violations, 0 actionable Fowler smells. Stable targets, reply-driven tab reconcile, form/IPC ownership, weak callbacks, navigation guards and canonical AT-SPI traversal reviewed. Final source/QA review at `61f3d98`.

## Spec

Independent Spec/UX review: 0 findings against FR-001 through FR-008. Prior visual review accepted error/Retry/progress and labeled compact tools. Final settled-capture review is recorded in the delivery handoff.

Standards: 0; Spec: 0. No outstanding review objections.

## Limits

Optional native AT-SPI headless attempts failed: initial legacy iterator TypeError was fixed in the QA adapter; subsequent attempts stopped at GTK display startup with `Failed to open display`. No native desktop/screen-reader pass is claimed. Manual OSK/IME/chooser/notifications/compositor/paid-provider flows not run. Xvfb and application-class high contrast do not certify desktop assistive technology. There is no automatic retry after an ambiguous transport failure.
