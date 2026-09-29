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

## Gotchas

The cat fixture echoes terminal input; it is not a live provider session. Reading
an approval preserves attention until an answer or agent transition resolves it.
