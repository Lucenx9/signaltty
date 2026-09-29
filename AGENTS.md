# AGENTS.md

signaltty: native Linux workspace for parallel AI coding agents. Rust workspace (edition 2021, rust 1.85): JSONL-over-Unix-socket server owns PTYs/state, GTK4/libadwaita GUI + CLI are clients.

## Layout

- `crates/signaltty-{core,proto,term,agent,plugin,testkit,server,cli,gui}/` — core/proto stay toolkit-free (ADR-0004); GUI is the only gtk/vte user.
- `docs/README.md` indexes the rest: `02` data model, `06` GUI toolkit, `07` agents, `08` IPC, `14` product direction, `15` agent skills. Read the doc for the area you touch before editing it.
- `docs/adr/` records decisions; new load-bearing choice → add an ADR there.
- `.specify/` holds the constitution + spec/plan/tasks templates + scripts; feature work lives in `specs/<nnn>-<name>/` via `/speckit-specify` → `/speckit-plan` → `/speckit-tasks` → `/speckit-implement`.

## Direction

Build to `docs/14-product-direction.md`: cmux + herdr behavior, t3code visual quality. The constitution (`.specify/memory/constitution.md`) is binding; specs that contradict it justify the deviation in-spec.

## Automation

Fresh environment? Run this first (bash + git only):

```sh
scripts/setup-agent.sh all       # pinned skills + spec-kit commands + git hooks
```

Skills do the heavy lifting every phase — load them before working, per
`docs/15-agent-skills.md` (Specify → `grill-with-docs`; Plan → `architect`;
Implement → `implement` + `tdd` + `karpathy-guidelines` + `principle-*`;
Review → `code-review` + `interrogate`; Verify → `prove-it-works`; UI →
`frontend-design` + emilkowalski set). CI runs fmt + clippy + tests on every push.

## Commands

System deps for the GUI: gtk4, libadwaita, vte (via pkg-config).

```sh
cargo build --workspace
cargo test --workspace            # all green before finishing
cargo clippy --workspace --all-targets
cargo fmt --check
```

GUI tests needing a display are `#[ignore]`d; run them explicitly:

```sh
dbus-run-session -- cargo test -p signaltty-gui -- --ignored --test-threads=1 <name>
```

Server integration tests live in `crates/signaltty-server/tests/` and use `signaltty-testkit` (ephemeral socket + state dir per test).

## Conventions

- IPC: method handlers in `crates/signaltty-server/src/router.rs` decode typed params from `params.rs` (unknown fields ignored, mistyped → `BAD_PARAMS`); error codes come from `signaltty-proto::code`, never ad-hoc strings. New method → row in `docs/08-ipc.md` + integration test.
- State changes emit events: mutate `Store` through its transition methods so the emit stays paired.
- Core (`signaltty-core`) holds pure logic with unit tests in-module; no PTY, async, or OS calls there.
- GUI reconcile is structural: ratio-only layout changes move widgets in place, never rebuild terminals.
- Commits read `area: what` (`gui: …`, `layouts: …`); one concern per commit.
- Fix the one pre-existing clippy warning only if you touch that line; otherwise leave it.
