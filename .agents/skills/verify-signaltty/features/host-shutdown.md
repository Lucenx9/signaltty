# Host shutdown save

## Sub-features

logind `PrepareForShutdown`, the delay inhibitor, resume metadata in the
snapshot, retry when login1 is missing or restarted, and inhibitor release on
ordinary server shutdown.

## How to get to it (user POV)

Shut down or restart the Linux host while a workspace is live. After the next
server start, resumable panes still have their session id and resume command.
Nothing in the GUI calls this path.

## Driving it with real IPC

Run `cargo test -p signaltty-server --test logind`. The tests start a private
`dbus-daemon` and a fake `org.freedesktop.login1`. They never contact the
host bus and never power off the machine. A true signal or an already-true
`PreparingForShutdown` must write `snapshot.json` before the inhibitor peer
sees EOF. A false signal, a denied inhibit, a missing name and a replaced
service must leave the server up until a real shutdown, and `server.shutdown`
must close the inhibitor.

## Gotchas

The GUI probes do not cover this. A green workspace test says nothing about
the inhibitor. Power loss and SIGKILL can still skip the save.
