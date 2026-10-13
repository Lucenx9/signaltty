# Implementation Plan: Verified PR readiness

**Branch**: `orch/pr-readiness` | **Date**: 2026-10-13 | **Spec**: [spec.md](spec.md)

## Summary

Replace the GUI-only readiness rule with a pure core task-stage projection. Store one typed GitHub observation with its head, draft, mergeability, merge state, and attempt timestamps. Failed refreshes retain facts but record an error. Older responses cannot replace a newer attempt. Reevaluate the open board on its existing 30-second clock tick.

## Technical Context

Rust edition 2021, minimum 1.92, pinned development toolchain. Existing serde, chrono, tokio, GTK4, libadwaita, and gh dependencies suffice. Persistence remains the existing snapshot and Store transition mechanism. Tests use existing in-module domain tests, isolated server IPC with fake gh, CLI tests, and ignored GTK display tests. No new library or service is needed.

## Constitution Check

Spec and clarification review precede this plan. Domain logic remains toolkit-free and receives time from callers. TDD uses the repository's approved seams. Mutation stays paired with Store events. Additive fields are serde-defaulted. ADR-0034 records the new readiness rule and observation ordering. No constitutional deviation is needed. Design recheck passes the same gates.

## Project Structure

- `crates/signaltty-core/src/workflow.rs`: TaskStage and pure task_stage(task, now), plus PR readiness and freshness checks.
- `crates/signaltty-core/src/model.rs`: additive TaskPr fields and typed merge facts.
- `crates/signaltty-server/src/tasks.rs`: typed gh decoding, failed observations, ordered writeback, and read-only stage fields for task.get/list/pr_refresh.
- `crates/signaltty-gui/src/board.rs`: shared stage derivation and readable PR notes.
- `crates/signaltty-gui/src/app.rs`: existing clock tick updates the open board.
- `crates/signaltty-cli/src/main.rs`: stage in existing task get/list text and batch failure summaries.
- `crates/signaltty-server/tests/orchestrator_pr.rs`: real IPC regression scenarios.

## Design sketch

`TaskStage { Working, NeedsYou, InReview, ReadyToMerge, Done }`
`task_stage(&Task, DateTime<Utc>) -> TaskStage`
`TaskPr::ready_to_merge(DateTime<Utc>) -> bool`
`TaskPr::observation_fresh(DateTime<Utc>) -> bool`
`TaskPr { head_sha?, draft?, mergeability, merge_state, refresh_attempted_at?, refresh_error?, existing facts }`

A positive observation is fresh for 300 seconds, excluding the exact expiry boundary and future timestamps. Readiness additionally requires a nonempty head, draft=false, mergeability=mergeable, merge_state=clean, checks=passing|none, review=approved|none, and no refresh error. Github's CLEAN merge state accounts for repository restrictions that a reviewDecision aggregate alone cannot establish.

Refresh requests take an attempt timestamp before I/O. Writeback matches the current PR URL and refuses an older attempt. A single failure persists invalidation before returning the original error. A batch returns successful task views and an additive errors array. Missing or mistyped required gh fields fail decoding. Unknown merge enum values map to unknown and do not establish readiness.

Task read views add a computed `stage` field to the returned task JSON. Snapshots, events, and persisted Task do not store the projection. GUI computes the same stage locally so time expiry needs no server event.

## Implementation strategy

Complete US1 domain regressions and migrate the board. Then complete US2 observation and expiry behavior. Finally expose US3 task views and CLI text. Verify each slice, run the complete project gate, inspect light/dark GTK renders, and open one focused PR. Automatic feedback and runtime survival remain separate features.
