# Implementation Plan: Self-Printing Schema

**Branch**: `002-self-printing-schema` | **Date**: 2026-09-29 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-self-printing-schema/spec.md`

## Summary

Add canonical `ALL` slices to `signaltty-proto` (`method::ALL`,
`event::ALL`, `code::ALL`), a `server.schema` router handler built
from them, a `signaltty schema` CLI command, a `docs/08-ipc.md` row,
and an integration test proving schema↔router sync by probing every
listed method (except `server.shutdown`).

## Technical Context

**Language/Version**: Rust 1.85, edition 2021

**Primary Dependencies**: none new (serde_json, clap already used)

**Storage**: N/A

**Testing**: proto unit test (`ALL` membership + no duplicates),
server integration test via `signaltty-testkit` (schema sync probe),
CLI e2e follows existing `crates/signaltty-cli/tests/cli.rs` pattern
only if trivial — the integration test plus manual `schema` run
suffice; `server.schema` needs no PTY.

**Target Platform**: Linux (server + CLI, no GUI deps)

**Project Type**: JSONL-over-Unix-socket daemon + CLI client

**Performance Goals**: One extra match arm + one small JSON object;
no hot path.

**Constraints**: Single source of truth — the router matches on the
same constants the schema lists; error codes from
`signaltty-proto::code` only; typed params pattern N/A (no params).

**Scale/Scope**: 4 source files + 1 doc + 1 test (~80 lines).

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- I (spec-driven): spec + plan + tasks before code. PASS
- II (product bar): directive 4 self-printing schema; enables
  directive 5 (agents read the contract at runtime). PASS
- III (skills): `architect` (this sketch), `tdd`, `karpathy-guidelines`
  (additive only, no router refactor). PASS
- IV (test-first): proto `ALL` test + integration sync test written
  before/with the handler; CLI verified by run. PASS
- V (typed boundaries): no params → no `params.rs` change; codes from
  `signaltty-proto::code`; handler returns opaque data (no ids). PASS
- VI (decisions on record): `docs/08-ipc.md` row added in the same
  change; no ADR (additive introspection, not load-bearing). PASS
- VII (simplicity): three slices + one arm + one CLI variant; no
  generic framework, no per-method metadata. PASS

No violations; complexity tracking N/A.

## Project Structure

### Documentation (this feature)

```text
specs/002-self-printing-schema/
├── spec.md               # feature specification
├── plan.md               # this file
└── tasks.md              # implementation tasks
```

### Source Code (repository root)

```text
crates/signaltty-proto/src/lib.rs        # method/event/code ALL slices + tests
crates/signaltty-server/src/router.rs    # SERVER_SCHEMA const use + h_server_schema + arm
crates/signaltty-cli/src/main.rs         # Schema variant + arm
docs/08-ipc.md                           # methods table row
crates/signaltty-server/tests/integration.rs  # schema_sync_probe test
```

**Structure Decision**: Existing workspace layout; each layer touched
once at its natural seam.

## Complexity Tracking

N/A — no constitution violations.
