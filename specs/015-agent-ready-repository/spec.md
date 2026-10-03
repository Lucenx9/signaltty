# Feature Specification: Agent-ready repository

**Branch**: `015-agent-ready-repository` | **Created**: 2026-10-03
**Status**: Clarified
**Input**: "Manda PR con tutto", following the repository assessment and primary-source research.

## User Scenarios & Testing

### US1 — Start and verify from a fresh checkout (P1)

An agent can prepare a supported Linux environment and run the same checks as CI.
Independent test: bootstrap a clean environment, run the doctor and full verification,
then inspect the machine-readable result and retained logs.

Acceptance: missing dependencies cause actionable failures; repeated setup converges;
failed checks return nonzero; ordinary and display tests both run; evidence identifies
the revision, commands, outcomes and limits. Existing desktop sessions remain untouched.

### US2 — Preserve architectural and visual quality (P1)

A contributor receives a failing check when dependencies violate the existing crate
boundaries or a new compiler warning appears. GUI checks exercise actual widgets.
Independent test: inject a forbidden dependency into a fixture and a new warning into
a diagnostic stream; both fail. Run the existing display suite with isolated sessions.

Acceptance: existing warning exceptions are narrowly identified; GTK display tests
run individually, produce available screenshots and never silently skip missing tools.
Manual accessibility and live-provider checks are explicitly distinguished from CI.

### US3 — Resume and compare agent work (P2)

Agents in supported harnesses discover one canonical verification guide. Contributors
use isolated worktrees and concise handoffs, and can compare process changes against
a small bank of representative tasks with recorded outcomes.
Independent test: resolve each harness skill entry to the canonical source; inspect
the benchmark task bank, reporting schema and requirement-to-verification mapping.

## Requirements

- FR-001: Provide repeatable dependency setup and read-only environment diagnosis.
- FR-002: Provide one verification entry point with explicit fast/full/desktop scope.
- FR-003: CI runs full verification and retains evidence even when a step fails.
- FR-004: Enforce documented toolkit/domain/client dependency boundaries.
- FR-005: Reject new warnings while retaining only reviewed existing exceptions.
- FR-006: Each display test runs in an isolated process and session; evidence includes available native renders.
- FR-007: Publish one canonical verification skill discoverable by supported skill-based harnesses.
- FR-008: Keep task routing concise and conditional, with reproducible handoffs and worktree isolation instructions.
- FR-009: Provide five representative development evaluation tasks and a result format; do not fabricate measured improvements.
- FR-010: Record visual/accessibility criteria, coverage limits and acceptance-to-proof mapping.
- FR-011: Declare and check a Rust minimum compatible with the locked dependency graph.

## Success Criteria

Every mandatory verification step reports a command and outcome. A clean supported
environment can compile and run the complete automated suite. Invalid dependency
fixtures and unapproved warnings fail. All discovered ignored GUI tests execute.
No missing prerequisite or unperformed manual check is reported as passed.

## Assumptions and scope

User authorization covers implementation, local verification, commit, push and PR.
The existing product is unchanged. Existing QA helpers and tests remain the primary
behavioral seams. Tool CLI boundaries are the seam for new tooling tests.
Ubuntu 26.04 is the CI/container reference. Native desktop probes remain an explicit
additional mode; CI cannot certify paid provider sessions, IME or desktop integrations.
The locked GTK dependencies require Rust 1.92, overriding the obsolete 1.85 claim via
a versioned constitution amendment and ADR. No GTK downgrade is in scope.
