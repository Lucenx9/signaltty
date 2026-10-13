# ADR-0034: Verified PR readiness

Status: proposed, 2026-10-13. Spec: [056-pr-readiness](../../specs/056-pr-readiness/spec.md).

## Context

The board previously inferred readiness from checks, reviews, and any successful refresh timestamp. It lacked draft status, mergeability, the observed head, and expiration. Failed refreshes retained positive cached state. Coordinating agents could not read the board's GUI-only stage.

## Decision

Store GitHub head, draft, mergeability, and merge-state facts alongside existing PR facts. Require a successful observation younger than five minutes, non-draft status, a known head, MERGEABLE, CLEAN, and satisfied checks and review before deriving Ready to merge. Missing facts and future timestamps remain In review. A conflicting PR needs attention.

Keep TaskStage as a pure core projection that receives the evaluation time. Task get/list/refresh responses expose a computed stage; snapshots and events store facts only. GUI derives the same projection and reevaluates the open board on its existing 30-second clock tick.

A refresh failure records error evidence while retaining the last successful facts. It immediately withdraws positive readiness. Batch refreshes report failures separately and continue successful observations. Attempt timestamps order concurrent writeback, and PR URL matching rejects observations for a replaced PR.

## Consequences

Old snapshots load with conservative defaults and need a new refresh before claiming readiness. The five-minute window describes a cached observation and cannot prove that remote state has not changed since then. Readiness is advisory and never authorizes a merge. Existing gh authentication and on-demand refresh remain in use. Detached PTYs, automatic feedback, and report delivery are separate decisions.
