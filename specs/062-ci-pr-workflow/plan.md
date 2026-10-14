# Implementation plan: CI and pull request workflow

**Branch**: `t3/ci-pr-workflow` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

Restrict automatic push checks to main, keep PR merge checks and add explicit
manual dispatch. Identify concurrency by workflow/event/PR or ref. Add an
independent workflow-lint job using actionlint 1.7.12 and an explicit runner label.
Preserve the existing rust and minimum-rust jobs. Add a concise PR template and
one delivery guide linked from the development index and workflow context.

## Technical context

YAML/Markdown only; GitHub Actions on Ubuntu 26.04; existing Cargo/Rust/native
verification. No application or script interfaces change. Workflow lint uses the
upstream versioned Docker image, including ShellCheck/pyflakes. Configuration
allows only the actual ubuntu-26.04 label missing from its built-in catalog.
Storage remains existing ignored evidence artifacts. Scope is CI and PR guidance.

## Constitution check

Spec clarified before plan and implementation. Existing command and linter seams
validate the change; no tests of a hand-written copy of GitHub's scheduler.
Observed duplicate cancellation is the failing baseline. A deliberately invalid
workflow provides a linter rejection case before valid configuration. Full
verification remains the delivery gate. No Rust, IPC, state, GUI or product
behavior changes. ADR-0038 records event/gate ownership. No exception needed.

## Design and ownership

- .github/workflows/ci.yml owns triggers, concurrency, permissions and jobs.
- .github/actionlint.yaml owns the additional recognized runner label.
- .github/pull_request_template.md prompts for evidence; it is not another gate.
- docs/19-ci-and-pull-requests.md owns review, failure recovery and merge procedure.
- .agents/workflow.json adds this guide to workflow context and catalogs feature062.
- Existing indexes remain generated; existing docs link to the new guide.

The root agent owns source writes. Grounding and Standards/Spec agents read only.
A new lint job supplements rather than replaces the common full gate. Avoid path
filters, per-branch lookup API scripts, mutable merge bots and new token secrets.
No repository settings are changed. Workflow dispatch becomes callable once the
workflow exists on main; new configuration is exercised by the follow-up PR.

## Verification strategy

Use actionlint1.7.12 at the actual workflow seam, including a deliberately invalid
expression/event fixture under ignored target before the fix. Review event and
concurrency examples against GitHub's contract. Exercise the upstream Docker image
when available. Inspect both independent reviews, commit final source, run shared
full once, check feature delivery against its report, push and open/link the PR.
GitHub CI validates hosted execution after publication. Report pending hosted
results honestly; no claim that static lint simulates GitHub's scheduler.

## Project structure

Feature artifacts: spec.md, plan.md, research.md, data-model.md, quickstart.md,
checklists/requirements.md, tasks.md and verification.md. This is internal
configuration, so no new public contract directory is required. Actual sources
are the existing .github workflow plus template/config and the delivery guide.
