# Approval

## Sub-features

Discovery, preserved decision on focus/attach, one-time choice delivery.

## How to get to it (user POV)

An agent requests approval. Click the attention count or use Ctrl+Shift+J, then
click Allow in the pane's inline decision bar.

## Driving it with GTK/IPC

Run `python3 scripts/qa-gui-reliability.py`. It creates a Codex-hinted cat fixture,
sends a production PermissionRequest hook, invokes `next-attention`, verifies the
same decision/gate, clicks the AT-SPI Allow action and observes decision consumption.
`decision_*` server tests additionally verify delivered bytes and stale answers.

`qa-ui-scenes.py --width 360 --long-choice --output /tmp/approval.png` captures
complete wrapped choices. The ignored `approval_question_and_choices_fit_a_narrow_pane`
test checks actual 320px GTK allocations, choice IDs and persistent VTE identity.
`keyboard_focus_renders_an_inactive_panes_controls` compares real rendered pixels;
`reduced_motion_removes_rendered_press_scaling` inspects native render transforms,
including pointer, reduced-motion and keyboard-focused activation.

## Gotchas

The cat fixture echoes terminal input; it is not a live provider session. Reading
an approval preserves attention until an answer or agent transition resolves it.
