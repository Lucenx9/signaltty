# Tasks: Agent Manifests + Live /proc Refresh

**Input**: `spec.md` + `plan.md` (this dir)

**Seams under test**: `agent::parse_manifest`; `OverlayAdapter` as
`AgentAdapter`; `Ctx::adapter` routing; `procscan` helpers against own pid;
socket-level promotion/cwd/malformed tests; `integration status` output.

## Phase 1: Setup

 - [x] T001 Add `toml = "0.8"` to `signaltty-agent/Cargo.toml` (workspace
  already pins it via `signaltty-plugin`).

## Phase 2: Foundational

 - [x] T002 [US1] `Manifest` + `parse_manifest` in
  `crates/signaltty-agent/src/manifest.rs` (RED: valid/legacy-compat/bad
  kind/bad enum/unknown-fields-ignored; GREEN).
 - [x] T003 [US1] `OverlayAdapter` + field-by-field fallthrough + unit tests
  (identify merge, lifecycle override vs builtin, session key, resume
  template, display override, channel passthrough).
 - [x] T004 [US1] `detect_kind_with_overlays` + `agents_dir()` in
  `paths.rs` (`SIGNALTTY_AGENTS_DIR` override) + `Config.agents_dir`.
 - [x] T005 [US2] `procscan.rs`: `deepest_descendant`, `proc_argv0`,
  `proc_cwd`, `scan` (change-only `pane.updated`) + unit tests against own
  pid (no fixtures that depend on machine state).

## Phase 3: US1 - Manifest overlays (P1) 🎯 MVP

 - [x] T006 [US1] Server startup load (isolated per-file failures, logged) +
  `Ctx::adapter()`; route hook-event/report-session/decision-answer/spawn
  detection through it.
 - [x] T007 [US1] Integration: manifest maps `sleep`→codex in temp agents
  dir → spawn promotes at spawn; hook override hook works; malformed file
  doesn't block startup.
 - [x] T008 [US1] `integration status` lists manifests (name/kind/file).

## Phase 4: US2 - Live /proc refresh (P2)

 - [x] T009 [US2] 10s tick in `server.rs` (`scan` → broadcast +
  `mark_persist` only when non-empty).
 - [x] T010 [US2] Integration: generic `sleep` pane with no manifest keeps
  kind but gets cwd; with manifest promotes to codex with one
  `pane.updated` (subscribe + assert single event); cwd change via
  `sh -c 'cd /tmp; sleep 30'` visible within 15s.

## Phase 5: Polish

 - [x] T011 Docs: `docs/07-agents.md` manifest section +
  `docs/adr/0009-agent-manifests.md`.
 - [x] T012 Gates: fmt, clippy (zero new), full tests; commits `area: what`.
