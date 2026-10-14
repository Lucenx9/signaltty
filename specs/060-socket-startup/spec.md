# Specification: Preserve occupied socket paths during startup

**Branch**: `t3/repo-bug-fixes-2`

**Created**: 2026-10-14

**Input**: Audit the repository and publish fixes for reproduced bugs.

## User scenario

A user starts the server with a socket path that is already occupied. Startup
must preserve unrelated files and existing listeners while still recovering a
socket left by a crashed server.

## Requirements and acceptance

- A regular file, directory, or symbolic link at the socket path is refused
  without removing it or changing its contents.
- An accessible live listener is refused without replacing its socket.
- Permission errors and other connection failures are reported without unlinking
  the socket. Only connection refusal establishes a stale socket for cleanup.
- An absent path can be bound; a stale Unix socket can be removed and rebound.
- Inspection uses the path entry itself, including dangling symbolic links.

## Assumptions and limits

The existing Linux Unix-socket transport and startup singleton policy remain
the contract. This correction does not introduce an interprocess startup lock
or claim atomicity against concurrent replacement of the path by another process.
No new IPC method or stored state is needed. Requirements are resolved.

## Success criteria

Production startup tests preserve file bytes, symbolic links, and live-socket
inodes on refusal. Real-server recovery serves `server.status` through a stale
socket path. The full repository verification gate passes.
