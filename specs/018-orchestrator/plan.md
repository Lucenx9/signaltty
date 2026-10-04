# Implementation plan: task orchestration

**Branch**: `orch/tasks` | **Date**: 2026-10-04 | **Spec**: `spec.md`

**Input**: `spec.md` + `research-codebase.md` + `research-external.md` +
`research-practices.md` (these three ARE the Phase 0 research; no unknowns
remain open).

## Summary

Add a persisted Task entity with an A2A-aligned lifecycle to the server, plus
the four orchestrator primitives around it: async start (sync cap → base →
worktree → spawn returning `pending`, server-owned background ready + gated
submit), gated prompt submit (herdr-style), cursor-based rendered reads (fixing
tail garbling at the root), turn-end-without-report → `input_required`,
structured result reports, and explicit merge/discard finish against the recorded
target branch (workmux rules). CLI parity
for all of it; minimal GUI row surfacing. MCP server, parent-inbox injection,
cost budgets, and kanban are deferred with in-spec reasons.

## Technical Context

**Language/Version**: Rust, edition 2021, minimum 1.92 (dev toolchain pinned in
`rust-toolchain.toml`)
**Primary Dependencies**: tokio, serde/serde_json, gtk4/libadwaita/vte (GUI
only), vt100, portable-pty, git CLI subprocesses
**Storage**: `snapshot.json` (+ `tasks` with serde defaults), `audit.jsonl`
journal, per-pane runtime scrollback ring (not persisted)
**Testing**: `cargo test --workspace`; server integration tests via
`signaltty-testkit` (ephemeral socket + state dir); GUI headless unit tests +
ignored display tests under xvfb
**Target Platform**: Linux (Ubuntu 26.04 reference)
**Project Type**: Native desktop app (server + GTK GUI + CLI)
**Performance Goals**: Submit gate + stall detection within documented budgets
(300 ms paste delay, 5 s activity gate); reads bounded (≤ 5000 lines/reply);
scrollback ring 5000 lines/pane
**Constraints**: One writer per state dir; per-path git lock; zero new clippy
warnings; snapshot backward-compatible (serde defaults)
**Scale/Scope**: One PR; default max 4 parallel tasks; attention listing
default limit 50

## Constitution Check

- I. Spec-driven: spec → plan → tasks before code; no `[NEEDS CLARIFICATION]`.
- II. Product bar: attention-first listing (directive 1), inline approvals
  reused not rebuilt (2), hooks-first + screen fallback (3), server owns tasks
  + versioned socket/CLI/`--json` + replay (4), agents-as-API-clients via
  `task.*`/`report`/`wait` (5), no new GUI views so no rendering-language risk
  (6), row chip via libadwaita patterns (7). No deviations.
- III. Skills: verify-signaltty skill governs delivery; 016 is the
  format/quality reference.
- IV. Test-first: E2E acceptance test written first (fails), then per-story
  red-green; integration tests for every new IPC method; GUI headless tests.
- V. Deep modules: `Task`/transitions pure in `signaltty-core`; typed
  `params.rs` decode; `Store` transition methods pair mutate+emit; GUI
  reconcile untouched structurally (row text only).
- VI. Decisions on record: ADR-0020 + rows in docs `02`/`08` for model/methods.
- VII. Simplicity: no inbox machine, no MCP server, no evaluator, no profiles,
  no kanban in this PR (all deferred in-spec); passive reads (no alt-screen
  refusal); branch kept by default.

GATE: PASS. Re-check after design: no new violations introduced below.

Round-1 re-check (async start, input_required-on-turn-end, data-dir worktrees,
recorded merge target, split test files, skill-embedded orchestrator loop): still
PASS — no new views, no inbox machine, no MCP server; the background step is
server-owned plumbing inside the existing task primitive, and the recorded
target only constrains the existing finish path.

## Project Structure

### Documentation (this feature)

```text
specs/018-orchestrator/
├── plan.md              # This file
├── research-codebase.md # Phase 0: what signaltty has (kept as-is)
├── research-external.md # Phase 0: how others do it (kept as-is)
├── research-practices.md# Phase 0: 2026 best practices (kept as-is)
├── data-model.md        # Phase 1: Task/Result/Contract/Pane additions
├── quickstart.md        # Phase 1: E2E validation guide
├── contracts/ipc.md     # Phase 1: methods/events/codes/CLI/MCP map
├── tasks.md             # Phase 2: dependency-ordered TDD tasks
└── checklists/requirements.md
```

### Source Code (repository root)

```text
crates/signaltty-core/src/      # model.rs (Task/Result/Contract), state.rs (TaskState),
                                # paths.rs (data_dir for worktree root)
crates/signaltty-proto/src/     # lib.rs (method/event/code constants)
crates/signaltty-term/src/      # headless.rs (scrollback + rendered ring + content_seq)
crates/signaltty-server/src/    # store.rs, persist.rs, router.rs, params.rs,
                                # pty.rs (env), git.rs, file_diff.rs, worktrees.rs, config.rs
crates/signaltty-server/tests/  # orchestrator_{e2e,submit,read,tasks,finish}.rs (NEW)
crates/signaltty-testkit/src/   # lib.rs (temp-repo + hook-driver fixtures)
crates/signaltty-cli/src/       # main.rs (task/*, report, submit, attention)
crates/signaltty-cli/assets/    # SKILL.md (orchestrator loop worked example)
crates/signaltty-gui/src/       # sidebar.rs (row chip), actor.rs (task.* events)
docs/adr/0020-task-orchestration.md
docs/{02-data-model,08-ipc}.md  # rows for Task + methods (implementers update)
```

**Structure Decision**: No new crates. Task logic lives in existing layers
(core pure → server owns → CLI/GUI are clients), per ADR-0004 and the
harmony rule that only the GUI uses gtk/vte.

## Interfaces and ownership

- Core owns `Task`, `TaskState`, transition validation (pure, unit-tested);
  server `Store` owns task storage + transition methods that pair mutation,
  emit, and persistence marking.
- `Snapshot.tasks` serde-defaulted; `persist::apply` restores tasks, fails
  non-terminal tasks with dead panes (evidence, once), and fails `pending`
  tasks whose background ready-wait died with the server (`stage: "restart"`).
- Router owns `task.*` / `pane.submit` / `attention.pending` handlers with
  typed params; the `task.start` background ready + submit step is server-owned
  (survives client disconnect; cancelled by cancel/discard/shutdown) and mutates
  tasks only via `Store` transition methods; git helpers own all subprocess calls (porcelain only, bounded
  deadlines, `LC_ALL=C`; never English-stderr matching).
- Term crate owns the rendered-line ring + `content_seq`; `pane.read` modes all
  derive from it.
- CLI is a thin wrapper (`--json` passthrough); GUI subscribes `task.*` and
  renders label + state on existing rows.
- E2E + failure-path tests own a temp-repo fixture and a synthetic-hook driver
  in testkit; the native-permission fixture pattern is reused for the
  block-and-answer leg.

## Complexity Tracking

No constitution violations to justify. The largest surface (finish safety
rules) is specified behavior copied from workmux/t3code, not new complexity.

## Validation

E2E scenario green (spec §Deterministic…); six failure-path tests green;
`scripts/verify.sh full`; light + dark GUI screenshots; independent diff
review before PR.
