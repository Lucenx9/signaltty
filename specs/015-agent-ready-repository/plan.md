# Implementation plan

## Grounding and design

Current setup installs agent tooling only. CI runs fmt, Clippy and ordinary tests.
QA helpers already isolate server state; GTK tests require distinct processes.
Current locked GTK packages declare Rust 1.92. Existing Clippy emits one distinct
warning in audit.rs, repeated for lib/test targets.

Two independent architect candidates were compared: Bash orchestration plus Python
validators, and Python standard-library orchestration plus the existing Bash setup.
Choose Python: structured Cargo output, durable evidence and process deadlines fit
one implementation better than shell pipelines plus JSON helper invocations.
Keep a thin verify.sh entry point, the Bash bootstrap and existing behavioral probes.
Adopt the shell candidate's explicit full/desktop distinction and exact warning baseline.

## Interfaces and ownership

- `scripts/setup-dev.sh`: idempotent Ubuntu package setup and pinned Rust components.
- `scripts/verify.sh doctor|fast|full|desktop [--output PATH]`: shared entry point.
- `scripts/verify.py`: preflight, ordered checks, command logs, deadlines and summary.
- `scripts/check-quality.py architecture|clippy`: Cargo metadata/diagnostic validation.
- `scripts/tests/`: public CLI behavior tests using isolated input fixtures.
- `.agents/skills/verify-signaltty`: canonical guide, linked from other skill harnesses.
- `docs/17-agent-development.md`: environment, isolation, handoff, GUI rubric and evaluation bank.

`fast` runs tooling tests, fmt, architecture, Clippy and ordinary workspace tests.
`full` adds build, server probes, every discovered ignored GTK test in a separate
Xvfb/D-Bus process, and refresh benchmark. `desktop` additionally invokes the native
accessibility probe; compositor-specific capture remains a documented explicit action.
Each run has a fresh artifact directory, per-command logs and a JSON summary including
revision/dirty state, durations, statuses and proof limits. A failed command stops the
run and retains its evidence. Child process groups are terminated on timeout/interruption.

## Constitution check

Spec precedes implementation; tests exercise tool CLI boundaries first, existing
server/GTK seams provide application proof. No new IPC, domain or UI behavior.
ADR-0017 amends minimum Rust and makes skill routing task-specific. Constitution
version advances with migration guidance for active specs. No product-bar deviation.

## Validation

Red/green CLI fixtures cover forbidden/aliased/transitive toolkit dependencies,
warning exceptions and new warnings, malformed inputs and runner failure propagation.
Run full verification locally and CI. Test minimum Rust separately. Inspect actual
native captures. Use an independent standards/spec review before creating the PR.
