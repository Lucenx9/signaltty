# Plan

Add a server-only logind module using zbus 5.19 with Tokio (already resolved
in the workspace) and futures-util StreamExt. Keep core/proto toolkit-free.
The monitor owns its connection, signal streams and inhibitor fd. A true
shutdown signal or initial property invokes the existing snapshot save,
closes the inhibitor, then notifies the server shutdown path. Retry bus or
owner failures at 1, 2, 4…60 seconds; ignore false signals. Disable property
caching so restart/setup reads the actual PreparingForShutdown value.

Own the monitor task with an abort-on-drop guard in serve, so normal API or
signal shutdown cannot leave an inhibitor behind. Keep the save callback
synchronous like the existing SIGTERM save, with no paid provider calls.

Test-first through the D-Bus boundary, with an isolated dbus-daemon and fake
login manager passing a real Unix fd. Tests observe EOF on its peer and the
saved snapshot, plus a real server subprocess for wiring/cleanup. Record
ADR-0028 and docs/09. Review Standards and Spec independently; run full
verification, minimum Rust, then publish/link the PR. No constitution deviations.
