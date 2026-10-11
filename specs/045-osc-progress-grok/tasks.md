# Tasks: OSC progress and Grok

## Setup and foundation
- [x] T001 Write and validate spec/plan/design artifacts in specs/045-osc-progress-grok/.
- [x] T002 Verify prerequisites with scripts/verify.sh doctor.

## US1 — progress evidence
- [x] T003 [US1] Red/green scanner progress, split boundaries and malformed reservation in crates/signaltty-term/src/osc.rs.
- [x] T004 [US1] Red/green explicit osc_progress region and absent evidence in crates/signaltty-agent/src/screen.rs; migrate classifier callers.
- [x] T005 [US1] Red/green real PTY progress retention/no notifications/explain in crates/signaltty-server/tests/screen_detection.rs and src/pty.rs, src/router.rs.
- [x] T006 [US1] Verify managed PTY reset/exit and hook authority through crates/signaltty-server/tests/screen_detection.rs.

## US2 — Grok
- [x] T007 [US2] Red/green Grok scoped classification and launcher discovery in crates/signaltty-agent/src/bundled.rs and screen/grok.toml.
- [x] T008 [US2] Real fake-Grok working/blocked/completed IPC fixture in crates/signaltty-server/tests/screen_detection.rs.

## Delivery
- [x] T009 Record ADR-0030 and update docs/07-agents.md, docs/05-terminal-backend.md, docs/08-ipc.md.
- [ ] T010 Independent Standards and Spec reviews against the pinned base, resolve findings.
- [ ] T011 Run scripts/verify.sh full; record proof and provider limits in specs/045-osc-progress-grok/verification.md.
- [ ] T012 Commit, push and open/link PR; verify registered link.

## Dependencies and strategy
T003 → T004 → T005 → T006; T007 needs T004; T008 needs T005/T007.
T009 → review → full verification → publication. Each behavior gets red before green.
Review axes can run in parallel after implementation; implementation remains sequential.
MVP is US1, then Grok extends the same evidence path.
