# Connection and window state

## Sub-features

Async interactions, outage recovery, single-window activation, restored status.

## How to get to it (user POV)

Use the app while the server waits, reopen it, or reconnect after a server restart.

## Driving it with GTK/Unix relay

Run `python3 scripts/qa-gui-reliability.py`. It relaunches and counts one window,
produces output while its relay is offline and after recovery, pauses its server
and calls GTK Actions.List within one second, then restarts and checks Exited.
Actor fake-socket tests verify offset overlap, response correlation, deadlines
and no mutation replay. `delayed_new_tab` verifies newer workspace navigation wins.

## Gotchas

The current VT screen is restored; this does not promise disconnected scrollback
history. Saved lifecycle metadata remains historical while live state determines
current presentation. GTK Peer.Ping is not a main-loop responsiveness probe.
