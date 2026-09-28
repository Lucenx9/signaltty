# ADR-0005 — Lifecycle and attention are independent axes

- Status: accepted
- Date: 2026-09-28

Context: conflating "agent is done" with "user has seen it" forces
false UI (working agents glow, reviewed dones still shout).

Decision: `Lifecycle` (unknown/working/blocked/done/idle/failed/
exited) and `Attention` (none/unread/input_required/
permission_required/warning/error) are separate enums on every pane,
transitioned by separate rules: adapters drive lifecycle, explicit
signals + user review drive attention. Clearing requires explicit
per-pane interaction.

Consequences: sidebar/ring logic keys off attention severity, never
off "working". Persistence preserves both across restarts.
