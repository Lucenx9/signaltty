# Implementation Plan: Priority-Sorted Sidebar

**Branch**: `001-priority-sorted-sidebar` | **Date**: 2026-09-29 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/001-priority-sorted-sidebar/spec.md`

## Summary

Add a sidebar sort rank to `Lifecycle` in `signaltty-core` (directive 1
order: blocked > failed > done > working > idle > unknown/exited), a
pure `sort_summaries` in `signaltty-gui/src/sidebar.rs` (severity →
rank → recency → name), and apply it in `App::apply_refresh` before
`Sidebar::update`. No IPC, store, or widget changes.

## Technical Context

**Language/Version**: Rust 1.85, edition 2021

**Primary Dependencies**: none new (gtk4/libadwaita already used by the GUI crate)

**Storage**: N/A (presentation order only, nothing persisted)

**Testing**: `cargo test -p signaltty-core` (rank unit tests),
`cargo test -p signaltty-gui` (sort unit tests, headless),
ignored display test under `dbus-run-session` with
`ADW_DEBUG_COLOR_SCHEME=prefer-light|prefer-dark`

**Target Platform**: Linux desktop (GUI crate)

**Project Type**: native desktop app (Rust workspace)

**Performance Goals**: Sort is O(n log n) over workspace count (<100);
unmeasurable vs one IPC roundtrip.

**Constraints**: Reconcile-by-id untouched; selection/scroll behavior
unchanged; zero new warnings (clippy/fmt gates).

**Scale/Scope**: 2 source files + tests (~60 lines).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- I (spec-driven): this spec + plan + tasks written before code. PASS
- II (product bar): implements directive 1's priority-sorted sidebar
  literally (blocked → done → working → idle). PASS
- III (skills): `architect` (this sketch), `tdd` (rank/sort tests
  first), `karpathy-guidelines` (surgical, no adjacent refactors). PASS
- IV (test-first): rank + sort tests written before implementation;
  display test proves reconcile intact. PASS
- V (typed boundaries): pure rank in core, sort over GUI summaries;
  no PTY/async/OS in core. PASS
- VI (decisions on record): order documented in spec; no ADR (not
  load-bearing — presentation order, reversible). PASS
- VII (simplicity): two small pure functions, no new types. PASS

No violations; complexity tracking N/A.

## Project Structure

### Documentation (this feature)

```text
specs/001-priority-sorted-sidebar/
├── spec.md               # feature specification
├── plan.md               # this file
└── tasks.md              # implementation tasks
```

### Source Code (repository root)

```text
crates/signaltty-core/src/state.rs      # Lifecycle::sidebar_rank + tests
crates/signaltty-gui/src/sidebar.rs     # sort_summaries + tests
crates/signaltty-gui/src/app.rs         # apply_refresh sorts before update (1 line)
```

**Structure Decision**: Existing workspace layout; changes land in the
two files that own the concepts (core owns lifecycle semantics, GUI
owns row presentation).

## Complexity Tracking

N/A — no constitution violations.
