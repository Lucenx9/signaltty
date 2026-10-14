# Feature Specification: Organized agent development workflow

**Feature Branch**: `t3/automate-agent-workflows`
**Created**: 2026-10-14
**Status**: Clarified
**Input**: Organize the repository and automate the development process for AI agents, using relevant skills and publish a PR.

## User scenarios and testing

### US1: Find current work and authoritative guidance (P1)

An agent arriving in a checkout discovers the selected feature, observed phase,
open tasks and relevant documents without guessing from directory ordering.
A contributor can browse every historical spec using its stable full slug.
Acceptance: read-only status and context commands produce human-readable or JSON
output, leave the worktree unchanged, and report malformed or missing metadata.
The spec index distinguishes recorded lifecycle from task completion claims.

### US2: Follow and resume a verifiable workflow (P1)

An agent receives a concrete next step based on actual prerequisites and can
prepare a handoff containing Git state, task references and verification status.
Acceptance: unresolved clarifications prevent advancement; missing artifacts give
an actionable command; checked tasks alone never mean verified delivery.
A handoff gathers facts without overwriting existing decisions or task status.

### US3: Verify the code that will be delivered (P1)

A contributor runs the existing verification and later checks whether its proof
still applies to this checkout. Acceptance: tracked, staged, unstaged and new
nonignored file changes invalidate applicable evidence; different input snapshots before and after a run
cannot produce passing evidence. Doctor is not delivery proof. Failures identify
the failed step, log and next diagnostic action.

### US4: Keep organization consistent (P2)

CI and local checks enforce the same routing, spec catalog, links and generated
index rules. Acceptance: broken authoritative links, missing catalog entries,
invalid lifecycle metadata and stale generated blocks fail with corrections.
Legacy specs remain valid without retroactively rewriting their contents.

## Functional requirements

- FR-001: Provide status, context, next, check and handoff commands through one
  stable entrypoint. JSON output has a documented version and actionable errors.
- FR-002: Resolve explicitly selected features before existing spec-kit selection;
  status/context/next/check MUST NOT persist or silently choose another feature.
- FR-003: Keep full spec slugs and existing paths stable. Explicit lifecycle is
  metadata; observed task progress is derived from tasks.md, never duplicated.
  Historical lifecycle defaults to unreviewed rather than claiming completion.
- FR-004: A canonical routing configuration supplies both task context and the
  generated task-routing table. Distinguish project-owned, pinned upstream and
  generated commands; preserve canonical verification symlinks.
- FR-005: Organize documentation indexes by current architecture, development
  process, decisions, research and historical evidence, without moving artifacts.
- FR-006: Check planning, implementation and delivery prerequisites, unresolved
  clarification markers and requirement/task traceability for enrolled features.
  Existing one-line/doc-only exemptions remain explicit; no new approval gates.
- FR-007: Record content provenance before and after verification and compare it
  with current content when checking delivery. Record mismatch or unavailable
  provenance honestly, preserve existing fresh evidence directories and logs.
- FR-008: Handoff generation is explicit, creates a new file only, and preserves
  task ownership and human decisions. Its facts are distinguishable from notes.
- FR-009: Repository checks run in the shared verification runner. Delivery check
  reports evidence status and applicable gate scope without rerunning tests by
  surprise. Test execution remains available through existing verify commands.
- FR-010: Failure guidance distinguishes environment/setup failure, command/test
  failure, interruption and stale proof. It directs bounded retries and new
  hypotheses instead of automatically retrying deterministic failures.
- FR-011: Extend the existing evaluation bank/protocol to compare workflow variants
  and multi-session resume. Do not claim improved agent quality without trials.
- FR-012: Document the source ownership decision in an ADR and align AGENTS.md,
  docs15/17 and verification guide with the new entrypoint.

## Edge cases

No feature selected; malformed metadata; explicit path outside specs or symlink
escape; missing task list; historical unnumbered tasks; duplicate numeric prefixes;
missing or running evidence; ignored generated files; index-only staging changes;
Git revision changes with identical files; content changing during checks; custom
evidence directory; CLI invoked outside repository root; existing handoff.

## Success criteria

- SC-001: A fresh session can identify relevant documents and a valid next step
  using one command, with stable JSON and no mutation of selection state.
- SC-002: CLI tests reject unresolved prerequisites, stale evidence and malformed
  inputs and preserve existing work and artifacts.
- SC-003: Every spec is listed exactly once under its stable full slug, and the
  repository check detects catalog or generated-table drift.
- SC-004: Existing full verification passes with the new tooling checks and
  provenance reporting. Paid provider and real-desktop proof limits stay explicit.

## Assumptions and clarification

All requirements are resolved from the conversation. This is repository tooling,
not application orchestration, scheduler work or product UI. Local Python standard
library is already supported. The user authorized the complete change, tests,
commit, push and PR. Public test seams are the CLI and verification evidence,
matching scripts/tests. No vendor command changes or historical ID renumbering.
