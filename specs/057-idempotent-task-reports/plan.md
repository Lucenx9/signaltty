# Implementation Plan: Accepted report retries

**Branch**: `orch/idempotent-task-reports` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

Compare repeated reports with the existing accepted result in core. Return whether a report changed the task, so Store emits its paired events only for a new result. The IPC handler returns the current task for either success and marks persistence only after a mutation.

## Technical Context

Rust 2021, minimum 1.92, development 1.94.0. Existing serde/serde_json and chrono types, Tokio server, JSONL Unix-socket IPC. Existing JSON snapshots store the accepted result. No dependencies, stored fields, methods, or GUI changes are added. Work is confined to core report semantics, Store event emission, the report handler, tests, and documentation.

Matching uses the existing typed fields: status, exact summary, ordered artifacts with versions, and serde_json evidence equality. Missing artifacts decode to an empty vector; missing/null evidence decode to None. `reported_at` is server-assigned and excluded. Existing result validation precedes matching.

## Constitution Check

Spec, plan, and tasks precede code. Core remains pure. Existing core model and real IPC tests are the agreed test interfaces. Terminal state remains immutable. All mutations and paired events stay in Store. ADR-0035 records retry semantics. Full verification is required. No constitutional deviation before or after design.

## Project Structure

- `crates/signaltty-core/src/model.rs`: `Task::apply_report` returns `Result<bool, CoreError>`; true means a new result, false means an identical accepted result.
- `crates/signaltty-server/src/store.rs`: `task_report` returns optional event pair and exits without mutation or emission for a retry.
- `crates/signaltty-server/src/tasks.rs`: retain selector resolution and response shape; persist only new reports.
- `crates/signaltty-server/tests/orchestrator_tasks.rs`: reconnect, concurrent reports, mismatch refusal, and snapshot-restored retry.
- `docs/02-data-model.md`, `docs/08-ipc.md`, `docs/adr/0035-idempotent-task-reports.md`: immutable result and retry contract.
- `specs/057-idempotent-task-reports/`: spec, decisions, contract, tasks, and verification handoff.

## Execution and Verification

Implement US1 as one failing core test followed by the minimal core fix, then one failing IPC replay/event test followed by Store and handler integration. US2 adds field-specific mismatches and terminal-without-result checks. Test concurrency and restart through the existing TestServer fixture. Run formatting and focused tests, then `scripts/verify.sh full`. Review the fixed-base diff along Standards and Spec axes, followed by Gemini 3.8 Flash High on Antigravity. Resolve valid findings before publishing the PR.

## Complexity Tracking

No new infrastructure. An optional event pair distinguishes a no-op from the existing paired event mutation. The accepted result itself is the retry identity; an additional idempotency key or outbox is unnecessary for this bounded contract.
