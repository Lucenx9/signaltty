# ADR-0041: Antigravity task execution and screen interruptions

Status: accepted, 2026-10-16.

## Context

Antigravity 1.4.0 permission screens already classify as blocked. Their tasks
remained working because only native decisions parked tasks. Antigravity also
required explicit worker argv, preventing default worker permission inheritance.
The user selected inheritance when the parent already runs automatically.

## Decision

Accept Antigravity binary aliases as task defaults while retaining generic
screen detection. Default and bare explicit Antigravity commands inherit the
native automatic prefix from a recorded automatic Codex/Claude/Antigravity
parent. This is the narrow exception to ADR-0036's literal explicit worker argv
rule. Worker options and all other explicit commands stay authoritative. Resume
keeps its existing same-provider rules.

Store pairs screen-blocked task transitions with task.updated events and
screen_blocked evidence. Initial submit reconciles an already visible blocker.
New working/finished activity resolves only the screen interruption. Native
decisions and accepted terminal results retain priority.

## Consequences

An automatic coordinator can start an Antigravity worker without a separate
permission flag recipe. Configured worker argv still needs its own permission
flags. A remaining native prompt becomes observable through task.wait and the
task board, without inventing answerable native decisions or approving a live
prompt. Fixtures prove PTY/IPC behavior; authenticated paid turns are not proof.

See [spec 071](../../specs/071-agy-task-orchestration/spec.md).
