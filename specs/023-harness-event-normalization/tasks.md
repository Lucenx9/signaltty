# Tasks

- [x] T001 Load area docs, skills and primary event contracts; write spec and plan in specs/023-harness-event-normalization/.
- [x] T002 [US1] Execute installed plugin through V2 setup in crates/signaltty-integration/tests/configuration.rs; capture failing error normalization before fix.
- [x] T003 [US1] Correct failure and nested error normalization in crates/signaltty-integration/src/lib.rs; verify adapter/server semantics.
- [x] T004 [US2] Add a failing V1 creation regression through server() in crates/signaltty-integration/tests/configuration.rs, then normalize info.id in src/lib.rs.
- [x] T005 Add Node to scripts/dev-packages.txt and scripts/verify.py; document test tooling and event handling in docs/07-agents.md and docs/17-agent-development.md.
- [x] T006 Run independent native Grok review and scripts/verify.sh full; record evidence in specs/023-harness-event-normalization/quickstart.md.
- [x] T007 Commit and create/link PR.

Dependencies: T001 → T002 → T003 → T004 → T005 → T008/T009 → T006 → T007.
Independent review is read-only and runs alongside implementation; one writer owns all edits.

- [x] T008 [US3] Reproduce stale blocked state in crates/signaltty-server/src/store.rs and tests/native_permissions.rs, then resume lifecycle through Store::answer_decision.
- [x] T009 Stabilize cairo focus readiness in crates/signaltty-gui/src/app_tests.rs and set explicit Xvfb desktop geometry in scripts/verify.py; rerun exact display test and full gate.
