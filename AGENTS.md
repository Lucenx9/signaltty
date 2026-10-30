# AGENTS.md

signaltty: native Linux workspace for parallel AI coding agents. Rust workspace (edition 2021, minimum Rust 1.92, development toolchain in `rust-toolchain.toml`): JSONL-over-Unix-socket server owns PTYs/state, GTK4/libadwaita GUI + CLI are clients.

## Start here

Run from the checkout root:

```sh
scripts/agent.sh status --json
scripts/agent.sh context --task docs --json  # choose a routing key for your task
```

Task keys include `research`, `implement`, `review`, `verify`, `gui`, `diagnose`,
`docs`, `core`, `ipc` and `workflow`; they are not free-form task names.
Pass `--feature <full-slug>` when work was selected explicitly. Inspection never
persists selection. A selected feature or completed checkbox is context, not proof
that it belongs to the current request or that its implementation still passes.
Load the routed skills and documents before editing.

## Layout

- `crates/signaltty-{core,proto,term,agent,integration,plugin,testkit,server,cli,gui}/` — core/proto stay toolkit-free (ADR-0004); GUI is the only gtk/vte user.
- `docs/README.md` indexes the rest: `02` data model, `06` GUI toolkit, `07` agents, `08` IPC, `14` product direction, `15` agent skills. Read the doc for the area you touch before editing it.
- `docs/adr/` records decisions; new load-bearing choice → add an ADR there.
- `.specify/` holds the constitution + spec/plan/tasks templates + scripts; feature work lives in `specs/<nnn>-<name>/` via `/speckit-specify` → `/speckit-clarify` → `/speckit-plan` → `/speckit-tasks` → `/speckit-implement`. Documentation-only edits and one-line fixes are exempt from feature artifacts; use the matching `--scope` in `docs/18-agent-workflow.md`.

## Direction

Build to `docs/14-product-direction.md`: cmux + herdr behavior, t3code visual quality. The constitution (`.specify/memory/constitution.md`) is binding; specs that contradict it justify the deviation in-spec.

## Automation

Fresh environment? Run this first (bash + git only):

```sh
scripts/setup-agent.sh all       # pinned skills + spec-kit commands + git hooks
scripts/verify.sh doctor         # Rust, native libraries and display-test tools
```

Use `docs/18-agent-workflow.md` for catalog enrollment, phase checks, handoffs
and failure recovery; `docs/15-agent-skills.md` routes the applicable skills.
Before completing a change, read `.agents/skills/verify-signaltty/SKILL.md`.
Before launching servers or GUI automation, read `docs/17-agent-development.md`
for process ownership, isolated sockets/state and D-Bus. The developer may be using
signaltty while you work; keep probes off the live instance and stop only processes
you created. That guide also owns setup, handoffs and proof limits.

## Commands

System deps for the GUI: gtk4, libadwaita, vte (via pkg-config).

```sh
scripts/verify.sh fast            # tooling, fmt, architecture, warning policy, build/tests
scripts/verify.sh full            # also server QA, isolated GTK tests and benchmark
```

GUI tests needing a display are `#[ignore]`d; run them explicitly:

```sh
GTK_A11Y=none xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui <name> -- --exact --ignored --test-threads=1
```

Server integration tests live in `crates/signaltty-server/tests/` and use `signaltty-testkit` (ephemeral socket + state dir per test).

## Completion

- For behavior changes, follow the test-first constitution and the testing guidance in `docs/17-agent-development.md`. Check applicable GUI, CLI, IPC and agent paths, including recovery and reverse transitions, using that guide's checklist.
- Stage final source, run `scripts/verify.sh full`, inspect emitted renders and check fresh delivery evidence as described in `docs/18-agent-workflow.md`. Fast checks and checked tasks do not replace the full local/CI gate, including for documentation-only changes.
- Before publishing or handling reviews, read `docs/19-ci-and-pull-requests.md` and use `.github/pull_request_template.md`. Report actual commands, results, evidence paths and unverified behavior; record missing checks as `not run`.
- Keep this file focused on repository-wide instructions. When changing agent guidance, follow the maintenance rules in `docs/18-agent-workflow.md`; edit authoritative sources rather than generated copies.

## Conventions

- IPC: method handlers in `crates/signaltty-server/src/router.rs` decode typed params from `params.rs` (unknown fields ignored, mistyped → `BAD_PARAMS`); error codes come from `signaltty-proto::code`, never ad-hoc strings. New method → row in `docs/08-ipc.md` + integration test.
- State changes emit events: mutate `Store` through its transition methods so the emit stays paired.
- Core (`signaltty-core`) holds pure logic with unit tests in-module; no PTY, async, or OS calls there.
- GUI reconcile is structural: ratio-only layout changes move widgets in place, never rebuild terminals.
- Commits read `area: what` (`gui: …`, `layouts: …`); one concern per commit.
- Clippy warnings must match the reviewed `scripts/clippy-baseline.json`; never regenerate the baseline to silence a warning.
