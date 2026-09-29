# ADR-0010: One Socket + Handles; Per-Session Sockets Deferred

**Status**: Accepted (2026-09-29) · **Spec**: `specs/006-agent-skill-handles-audit/`

## Context

herdr runs named sessions with per-session sockets; signaltty runs one
Unix socket with non-unique workspace names. Agents-as-clients (directive
5) need stable typable addresses, and operators need durable event
history.

## Decision

1. **Named handles on one socket.** Immutable unique slugs (`my-api`,
   `my-api-2`) accepted everywhere a workspace id is. The workflows
   per-session sockets serve (attach this session, list sessions, resume
   that one) all work over handle-or-id today.
2. **Per-session sockets deferred.** A second listener family would double
   the auth (dir/socket modes), snapshot (which socket owns what), and
   CLI routing surface for no proven demand. Revisit with evidence: two
   clients needing different socket permissions, or a session that must
   outlive/move across servers.
3. **Audit instead of bigger rings.** The 1024-event ring stays a live
   buffer; durability is the JSONL file with rotation + restart backfill.

## Consequences

- `signaltty new --name X` prints the handle; scripts pin handles, humans
  read names.
- `restart()` respawns on the same socket + state dir, so handles and the
  audit survive restarts by construction.
