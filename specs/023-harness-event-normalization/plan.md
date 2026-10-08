# Implementation Plan: Harness event normalization

Branch: `t3/harness-integration-fixes` · Date: 2026-10-08 · Spec: [spec.md](spec.md)

## Summary

Repair OpenCode provider event normalization in the shared installer shim.
Retain the existing pure adapter and IPC contracts. Execute installed source
through both public plugin entrypoints with an isolated fake reporter.

## Technical Context

Rust 2021/minimum 1.92; generated JavaScript; Linux. Existing serde/nix
integration library, existing server testkit. Node required for behavioral
plugin tests, added to reference setup and doctor. No paid provider requests.

## Constitution Check

Spec and clarification review complete, then plan/tasks before implementation.
Tests use existing public installer and hook-event seams, red before green.
No OS calls in pure adapters; no model or architecture changes, so no new ADR.
Accurate attention states follow cmux/herdr; existing native presentation remains.

## Structure and sequence

1. Regression test on installed V2 setup → error translation fix.
2. Regression test on installed V1 server → identity normalization fix.
3. Test unknown events, success, unmanaged-pane no-op and unload cancellation.
4. Document contracts and Node test dependency, independent review, full gate.

## Risks

Event fixtures are grounded in tagged provider schemas, but do not certify a
paid live session. Node tests verify actual generated JavaScript, not source
substrings. Reporter timeout remains bounded and user config stays untouched.

## Audit refinement: answered approvals

Grok found that Store::answer_decision resumes a task but leaves a blocked pane
blocked. Add a Blocked → Working transition there, shared by native and TypeText
answer paths; retain stale-answer and non-answer clear behavior. Test first at
Store and native reporter IPC seams. Stabilize the existing GTK focus test by
waiting for mapping before its focus assertion with the shared bounded wait helper.

Display validation refinement: the host Xvfb default is 640×480. Explicitly set
1920×1080×24 for GTK scenes and refresh benchmark so desktop-width assertions
remain meaningful across distributions; no production GUI behavior changes.
