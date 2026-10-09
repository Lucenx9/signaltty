# ADR-0028: Save resume metadata before logind shutdown

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/032-logind-shutdown/`.

## Context

A host shutdown can stop agent processes before SIGTERM reaches the server.
logind emits `PrepareForShutdown` early and can hold a delay inhibitor while
a process finishes work. The server already saves its snapshot on SIGTERM,
Ctrl-C and `server.shutdown`. That save is too late when the panes are already
gone. The server must still start when logind is missing or denies the lock.

## Decision

1. **Best-effort monitor, owned by `serve`.** The server subscribes to
   `PrepareForShutdown` on the system bus, takes a `shutdown`/`delay` inhibitor,
   then reads `PreparingForShutdown`. Startup does not wait on this. Bus
   failures, a denied inhibit and a lost name owner retry at 1, 2, 4… seconds,
   capped at 60. Property caching stays off so a reconnect reads the live value.
2. **Save, then release, then stop.** A true signal or an already-true property
   writes the existing snapshot and scrollback, closes the inhibitor, and uses
   the normal shutdown path. A false signal does not stop the server. A failed
   save is logged and still releases the lock and stops the server.
3. **Every exit drops the monitor.** Aborting its task closes the inhibitor, so
   API shutdown, SIGTERM and Ctrl-C cannot leave the delay held.

## Consequences

The delay lasts only as long as logind allows. Power loss and SIGKILL can still
skip the save. There is no suspend handling, auto-resume, or stronger snapshot
durability. Core and proto stay free of D-Bus. Tests use a private bus and a
fake login manager; they never shut down the host. Every other automated
server sets `SIGNALTTY_LOGIND=0` so it cannot hold the host delay lock.
