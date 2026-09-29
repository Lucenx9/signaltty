# signaltty Constitution

## Core Principles

### I. Spec-driven, always

Every feature starts as `specs/<nnn>-<name>/spec.md` (via `/speckit-specify`),
then `plan.md`, then `tasks.md`, then code. No implementation without a spec;
no spec without clarified requirements (`/speckit-clarify` until no
`[NEEDS CLARIFICATION]` markers remain). One-line fixes and doc-only edits are
exempt; everything else follows the flow.

### II. Product bar: cmux + herdr, t3code visuals (NON-NEGOTIABLE)

`docs/14-product-direction.md` is the acceptance bar. Attention-first shell,
inline approvals, server-owns-sessions, agents-as-API-clients, t3code-grade
rendering (collapse-and-count, per-turn diffs, verb-tense run states), and
Linear-grade tokens through libadwaita patterns. A spec that contradicts a
directive must justify the deviation in the spec itself.

### III. Skills are load-bearing tooling

The agent loads the phase's skills from `docs/15-agent-skills.md` before
working, not after: Specify → `grill-with-docs`/`to-spec`; Plan →
`architect`/`domain-modeling`/`codebase-design`; Tasks → `to-tickets`;
Implement → `implement` + `tdd` + `karpathy-guidelines` + the applicable
`principle-*`; Review → `code-review` + `interrogate` + `blast-radius`;
Verify → `prove-it-works` + `create-verification-skill`;
UI work → `frontend-design` + the emilkowalski set;
stuck → `diagnosing-bugs`/`figure-it-out`.
If the skills are not installed in the current environment, run
`scripts/setup-agent.sh all` first — it works from any checkout with only
bash + git.

### IV. Test-first (NON-NEGOTIABLE)

TDD red-green-refactor. Core logic gets in-module unit tests; new IPC methods
get server integration tests (`signaltty-testkit`, ephemeral socket + state
dir per test); GUI behavior gets headless unit tests, GTK display tests only
when no other seam exists. `cargo test --workspace` green before finishing.

### V. Deep modules, typed boundaries

Pure logic lives in `signaltty-core` (no PTY/async/OS). IPC params decode
through typed `params.rs` (unknown fields ignored, mistyped → `BAD_PARAMS`,
codes from `signaltty-proto::code`). State mutates only through `Store`
transition methods so emits stay paired. GUI reconcile is structural: never
rebuild terminals for ratio-only changes.

### VI. Decisions on record

New load-bearing choice → ADR in `docs/adr/` + row in the touched doc
(`08-ipc.md` for methods, `02-data-model.md` for model changes). Specs stay in
`specs/` as history; ADRs distill what survived.

### VII. Simplicity over scaffolding

Smallest change that solves the problem (`laziness-protocol`,
`subtract-before-you-add`). No compat shims, no speculative providers, no
per-compositor special cases. Delete dead code in the same commit that kills it.

## Quality Gates

- `cargo fmt --check`, `cargo clippy --workspace --all-targets` (zero new
  warnings), `cargo test --workspace` — all green before finishing.
- Rust 1.85, edition 2021. Commits read `area: what`, one concern per commit.
- GUI changes are verified with light + dark screenshots
  (`ADW_DEBUG_COLOR_SCHEME=prefer-light`) before finishing.

## Governance

This constitution supersedes all other practices. Amendments require a version
bump below, a migration note for in-flight specs, and an ADR when the change is
load-bearing. All reviews verify compliance; complexity must be justified
against Principle VII.

**Version**: 1.0.0 | **Ratified**: 2026-09-29 | **Last Amended**: 2026-09-29
