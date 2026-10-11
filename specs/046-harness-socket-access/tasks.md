# Tasks

## Setup and foundations
- [x] T001 Reproduce the native Codex socket denial and record sources in research.md.
- [x] T002 Specify/clarify and plan feature in specs/046-harness-socket-access/.

## US1: workspace access
- [x] T003 [US1] Write failing real CLI/IPC bridge test in crates/signaltty-server/tests/mcp_bridge.rs.
- [x] T004 [US1] Implement stdio tool bridge in crates/signaltty-cli/src/mcp.rs and main.rs.
- [x] T005 [US1] Cover invalid context, errors, rejected streams and limits in mcp_bridge.rs.
- [x] T006 [US1] Add sandbox-denial guidance to crates/signaltty-cli/src/client.rs.

## US2: harness integration
- [x] T007 [US2] Write failing provider configuration cases in crates/signaltty-integration/tests/configuration.rs.
- [x] T008 [US2] Implement owned registration updates in crates/signaltty-integration/src/mcp.rs and lib.rs.
- [x] T009 [US2] Expose registration status in crates/signaltty-cli/src/integration.rs and launch notices.
- [x] T010 [US2] Verify provider overrides, conflicts, disabled entries and owned cleanup in configuration.rs.

## Delivery
- [x] T011 Add native sandbox/bridge QA in scripts/qa-harness-socket.py.
- [x] T012 Record ADR and harness support matrix in docs/07-agents.md and docs/08-ipc.md.
- [ ] T013 Run two-axis code review and full verification; record evidence in verification.md.
- [ ] T014 Commit, push, open and link PR; record handoff.md.

Dependencies: setup → US1 → US2 → delivery. Independent documentation research
can proceed while tests build; no shared-file parallel writes are needed.
MVP is a usable CLI bridge; delivery includes all stories.
