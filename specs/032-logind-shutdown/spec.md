# Specification: save before host shutdown

Created: 2026-10-09. Status: clarified. Product direction: directive 4.

When Linux shuts down, agents may exit before SIGTERM reaches the server.
Save the current workspace and resume metadata on logind's early shutdown
notification, while a delay inhibitor gives the save time to finish.

## Acceptance

1. Best-effort monitoring of org.freedesktop.login1 on the system bus does
   not delay server startup or make it depend on logind. Failed connections,
   denied inhibition and service restarts retry with bounded backoff.
2. Subscribe before acquiring a shutdown delay inhibitor and reading
   PreparingForShutdown. Handle both an already-active shutdown and
   PrepareForShutdown(true); false notifications never stop the server.
3. Save the existing snapshot and scrollback while holding the inhibitor;
   release it after the save attempt, then initiate normal server shutdown.
   Save errors are logged and still release the lock and stop the server.
4. Ctrl-C, SIGTERM and server.shutdown retain their behavior. When the
   server stops for any reason its logind monitor and inhibitor are released.
5. Tests use a private D-Bus service and temporary server state, never host
   shutdown. Cover saved resume metadata, inhibitor ordering, false signals,
   initial shutdown property, unavailable/denied service, reconnection and
   cleanup. Full verification and Rust 1.92 compilation pass before the PR.

## Scope and clarification

No suspend handling, auto-resume, snapshot durability upgrade or new IPC
method. Delay is bounded by logind's own policy; power loss and SIGKILL
remain outside these guarantees. The previously selected isolated D-Bus
and server persistence boundaries are the test seams. No open questions.
