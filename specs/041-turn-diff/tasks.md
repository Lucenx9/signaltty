# Tasks: Latest-turn changes

- [x] T001 Server integration tests first (`crates/signaltty-server/tests/turn_diff.rs`): capture on `working`, blocked continuation, committed + uncommitted turn work, earlier dirt excluded, `turn: null`, turn file read without a turn.
- [x] T002 `turns.rs`: baseline registry, private-index snapshot, capture task on `agent.working`, `workspace.turn_started`.
- [x] T003 `workspace.diff` / `workspace.file_diff` accept `scope`; tree-to-tree summary and file read; `workspace.close` forgets the baseline.
- [x] T004 CLI `workspace diff --turn` (extends `cli_workspace_diff_reports_counts`).
- [x] T005 GUI All / Turn toggle, scope wording, turn refreshes; headless tests plus GTK test `changes_panel_scopes_to_the_latest_turn` with light/dark captures.
- [x] T006 ADR-0029, `docs/08-ipc.md` rows and event, ADR-0011 status.
