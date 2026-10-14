# ADR-0037: Agent workflow sources and content-bound verification

Status: accepted, 2026-10-14.

## Context

Setup, Spec Kit and verification already own their respective operations. Agents
still reconstruct routing, selected work, prerequisites and evidence freshness
from prose. Historical specs reuse numeric prefixes; task checkboxes alone cannot
establish lifecycle or delivered behavior. Commit plus a dirty boolean cannot
identify changes made during or after verification.

## Decision

Add a stdlib Python workflow owner behind scripts/agent.sh. Keep a complete
full-slug catalog and task routing in .agents/workflow.json. Lifecycle and explicit
phase-check enrollment are recorded metadata; requirements, task progress and
next actions derive from existing feature files. Generate the catalog index and
routing table. Preserve historical paths and initially unreviewed lifecycle.

Reuse Spec Kit's read-only path-resolution interface without changing its vendored
sources. Setup owns rendering and checking generated commands. Repository checks
run inside the existing verification runner, so CI and local policy stay aligned.

Use a separate verification_evidence.py owner shared by verify.py and agent.py.
Hash actual working and staged inputs, record snapshots before/after checks, and
reject success after mutation. Compare content, modes and index identity rather
than Git revision alone; a commit of identical staged/working content keeps proof
applicable. Evidence output is ignored or outside the checkout, never a caller-
selected source exclusion. Unknown provenance is not passing delivery proof.

Handoff is an explicit exclusive-create operation with observed facts and a
separate decisions/ownership section. Saving a nonignored handoff changes source
identity. Phase checks do not execute tests or create extra permission gates.

## Consequences

One reviewed metadata file is required when adding a feature or changing lifecycle.
Historical incomplete specs do not block repository checks. Enrollment cannot
silently disappear through removal of a per-feature sidecar. Generated indexes
need regeneration when metadata or artifact presence changes, not each checkbox.

The central catalog won over per-feature sidecars; a mutable phase/task ledger
would duplicate existing facts. Two Python owners keep evidence policy separate
without a class or module for every phase. The API remains ordinary scripts,
without a new application orchestrator or runtime dependency.

Structural checks and content identity establish which inputs were exercised.
They do not certify semantic completeness, visual quality or native manual paths.
Evaluate instruction/routing changes using the task bank before claiming an agent
quality or productivity improvement. This extends ADR-0017 without changing the
constitution's feature exemptions or full delivery gate.
