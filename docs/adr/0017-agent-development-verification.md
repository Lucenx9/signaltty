# ADR-0017: Reproducible agent development and verification

Status: accepted, 2026-10-03.

## Context

Agent setup installed instructions but not application dependencies. CI omitted
ignored GTK tests and real-server QA. The declared Rust 1.85 minimum contradicted
the locked GTK packages, which require 1.92. Verification guidance was Cursor-only.

## Decision

Use a standard-library Python runner behind `scripts/verify.sh` for local and CI
checks. Retain existing server and native GUI probes. Each display test owns its
Xvfb/D-Bus process; preserve command logs and native renders in a per-run directory.
Keep native accessibility and compositor-specific evidence explicit.

Use Ubuntu 26.04 as the shared CI/container reference, a common package list,
Rust 1.94.0 for development and a separately checked minimum of 1.92. Cargo.lock
pins Rust dependencies; apt package revisions and base-image updates remain mutable.
This is a repeatable supported environment, not a bit-for-bit reproducible image.

Validate resolved dependency boundaries and every new warning. The sole legacy
warning is identified by lint, file, line and message in a reviewed baseline;
moving or changing that diagnostic requires review, never a blanket lint allowance.

Store one verification skill under `.agents/skills` and use relative links for
other skill-based harnesses. Route skills by task; retain project-specific rules
and test their usefulness with a small repeatable evaluation bank.

## Consequences

No product IPC or UI behavior changes. Full CI costs more than headless tests but
catches native regressions previously verified manually. Source OS-call purity,
visual taste, real desktop integrations and paid provider behavior still need
appropriate review. Constitution 1.1.0 replaces the stale minimum and broad skill
loading requirements; active specs use these delivery gates without rewriting history.
