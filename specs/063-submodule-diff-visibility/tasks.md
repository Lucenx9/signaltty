# Tasks: Submodule diff visibility

## Diagnosis and design

- [x] T001 [US1] [FR-002] Reproduce false unchanged output through tests/file_diff.rs and isolate Git suppression.
- [x] T002 [US1] [FR-001] [FR-003] Clarify spec, plan and requirement checklist under specs/063-submodule-diff-visibility/.

## Implementation

- [x] T003 [US1] [FR-002] Pin selected-file submodule comparison in file_diff.rs and pass the red IPC fixture.
- [x] T004 [US1] [FR-001] [FR-003] Cover and fix HEAD/task/turn summaries in git.rs and turns.rs with the real fixture.
- [x] T005 [US1] [FR-004] Verify unchanged pointers and stable index/config; document explicit comparison behavior in docs/08-ipc.md.

## Delivery

- [x] T006 [US1] [FR-001] [FR-002] [FR-003] [FR-004] Complete independent Standards/Spec reviews and prepare final source for shared full verification.

## Hosted CI recovery

- [x] T007 [US1] [FR-003] [FR-004] Reproduce Git 2.55 snapshot omissions and capture tracked gitlinks despite ignore=all using only the private index in turns.rs.
- [x] T008 [US1] [FR-003] [FR-004] Strengthen file_diff.rs regression for literal gitlink paths, ignored content and explicitly staged/unstaged pointers on Git 2.47 and 2.55.
- [x] T009 [US1] [FR-003] [FR-004] Review the CI correction independently and stage final source for fresh shared full verification.
