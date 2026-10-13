# Tasks: Accepted report retries

## Setup and contract

- [x] T001 Prepare the isolated checkout and doctor proof in `target/verification/`.
- [x] T002 Complete spec, clarified assumptions, plan, and contract in `specs/057-idempotent-task-reports/`.

## US1: Recover the acknowledgment

- [x] T003 [US1] Add a failing identical-report core regression in `crates/signaltty-core/src/model.rs`.
- [x] T004 [US1] Return whether a report mutates the task in `crates/signaltty-core/src/model.rs`, excluding acceptance time from content matching.
- [x] T005 [US1] Add failing reconnect/event IPC regression in `crates/signaltty-server/tests/orchestrator_tasks.rs`.
- [x] T006 [US1] Suppress duplicate paired events and persistence requests in `crates/signaltty-server/src/store.rs` and `crates/signaltty-server/src/tasks.rs`.
- [x] T007 [US1] Verify concurrent identical reports and saved-result retries in `crates/signaltty-server/tests/orchestrator_tasks.rs`.

## US2: Preserve the outcome

- [x] T008 [US2] Verify each content mismatch and terminal tasks without results in `crates/signaltty-core/src/model.rs` and `crates/signaltty-server/tests/orchestrator_tasks.rs`. Retain limits: summary 1 byte … 8 KiB; at most 32 artifacts.
- [x] T009 [US2] Record immutable retry semantics in `docs/02-data-model.md`, `docs/08-ipc.md`, and `docs/adr/0035-idempotent-task-reports.md`.

## Delivery

- [ ] T010 Run the full gate and record actual proof and limits in `specs/057-idempotent-task-reports/verification.md`.
- [ ] T011 Review Standards and Spec, then final Gemini 3.8 Flash High on Antigravity; record findings and responses in `specs/057-idempotent-task-reports/verification.md`.
- [ ] T012 Commit and open a linked PR; update `specs/057-idempotent-task-reports/handoff.md`.

Dependencies: T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012. US1 is the MVP; US2 locks down refusals before delivery. Tests precede each implementation slice. Documentation and read-only review can run independently after implementation; source edits have one owner.
