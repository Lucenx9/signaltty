# Plan: Incomplete PR checks

## Context and constitution

Rust 2021, minimum 1.92, existing pure core module. Read docs/02, docs/14 and ADR-0034. Preserve GitHub failure precedence and all known successful states. No OS, async or toolkit dependencies enter core. The existing readiness decision applies, so no new ADR is needed.

## Sequence

Write a failing regression through rollup_status_checks and the task_stage projection. Minimize each payload and rank falsifiable causes before fixing. Recognize a passing outcome explicitly; otherwise preserve pending unless any entry fails. Keep the public API and existing valid outcomes.

## Verification and delivery

Run core tests, independent Standards and Spec reviews against the pinned base, then stage final content and run scripts/verify.sh full. Inspect native renders and check fresh delivery evidence. Publish one linked PR and merge only after current checks and review decisions are settled. No paid provider or real desktop checks are claimed.
