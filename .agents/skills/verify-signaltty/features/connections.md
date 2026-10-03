# Connection and window state

## Sub-features

Async interactions, outage recovery, single-window activation, restored status.

## How to get to it (user POV)

Use the app while the server waits, reopen it, or reconnect after a server restart.

## Driving it with GTK/Unix relay

Run `python3 scripts/qa-gui-reliability.py`. It relaunches and counts one window,
produces output while its relay is offline and after recovery, pauses its server
and calls GTK Actions.List within one second, then restarts and checks Exited.
Actor tests use real Unix sockets with scripted peers. They verify overlap, forward
byte-gap recovery, preserved PTY size, response correlation, deadlines and no mutation
replay. `forward_output_gap_reconnects_and_replaces_the_screen` requires a fresh
snapshot and exactly one copy of later bytes. Real server integration tests cover
offset continuity across attach and streaming across exit/resume. The ignored GTK
event-batch test verifies full authoritative refresh after reconnect; structural
reconciliation retains VTE widgets. Independent worktree/file-diff sockets
keep normal control responsive. `delayed_new_tab` verifies newer navigation wins.
Event-cursor and new-work wait coverage is in [orchestration](orchestration.md).

## Gotchas

The current VT screen is restored; this does not promise disconnected scrollback
history. Saved lifecycle metadata remains historical while live state determines
current presentation. GTK Peer.Ping is not a main-loop responsiveness probe.
