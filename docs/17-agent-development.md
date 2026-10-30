# Agent development and verification

## Prepare a checkout

The supported reference environment is Ubuntu 26.04. Open the repository in its
development container, or run these commands on an Ubuntu host with rustup installed:

```sh
scripts/setup-dev.sh --system
scripts/setup-agent.sh all
scripts/verify.sh doctor
```

`setup-dev.sh --system` installs the packages listed in `scripts/dev-packages.txt`
and the toolchain from `rust-toolchain.toml`. It is safe to repeat. On another
distribution install equivalent packages and run it without `--system`.
The doctor reports missing dependencies without installing anything. Python 3.9+
is required by the verification runner. Node runs behavioral tests of the installed
OpenCode plugin (both provider entrypoints); it is development tooling, not a
Signaltty runtime dependency. The locked GTK packages require Rust 1.92;
development uses 1.94.0 and CI compiles all targets separately on 1.92.0.

To build the same container without an editor:

```sh
docker build -f .devcontainer/Dockerfile -t signaltty-dev .
docker run --rm -v "$PWD:/workspace:ro" -w /workspace signaltty-dev scripts/verify.sh doctor --output /tmp/doctor-evidence
```

The image runs as the unprivileged `ubuntu` user. Permission-sensitive tests require
an unprivileged user; running the suite as root changes their semantics. Use a
disposable checkout or a named workspace volume when host user IDs differ.
The Ubuntu tag and apt
packages receive updates; this setup is repeatable, not bit-for-bit reproducible.

## Inspect the agent workflow

Use `scripts/agent.sh status --json` to inspect selected work and
`context --task <key> --json` to load the applicable references. Full slugs
identify features; [the catalog](../specs/README.md) keeps lifecycle separate
from task checkboxes. [Workflow commands and ownership](18-agent-workflow.md)
describes enrollment, generated indexes, phase gates and failure recovery.

## Choose the verification scope

For GitHub CI events, PR descriptions, review comments and merging, use the
[CI and PR procedure](19-ci-and-pull-requests.md).

| Command | Proof |
|---|---|
| `scripts/verify.sh doctor` | Rust/components, compiler, native package versions, Xvfb and D-Bus prerequisites |
| `scripts/verify.sh fast` | Tooling regressions, formatting, architecture boundaries, warning policy, build and ordinary workspace tests |
| `scripts/verify.sh full` | Fast plus real-server edge cases, every ignored GTK test and refresh benchmark |
| `scripts/verify.sh desktop` | Full plus real desktop AT-SPI reliability probe |

The runner uses this checkout's `target/`, overriding an inherited `CARGO_TARGET_DIR`
so the existing QA helpers always execute the binaries just built. Concurrent work
uses separate checkouts. It does not contact the default application socket.

Each run prints a new evidence directory under `target/verification/`. Use
`--output /absolute/new-directory` for a chosen location; existing directories are
rejected to prevent stale results from being mistaken for new proof. `summary.json`
records revision, dirty state, tool versions, commands, durations and outcomes.
Versioned content/index provenance identifies the actual tracked and nonignored
new inputs before and after execution. Different input snapshots before/after the run prevent a
passing result. Stage final source first; committing identical content preserves
proof, while later content or index changes require a new run. Check a chosen run
with `scripts/agent.sh check --phase delivery --feature <full-slug> --evidence <run>`.
Custom evidence output must be ignored or outside the checkout.
Logs are saved before running the next step. On failure, read the failed step log
and `qa-logs/`. CI uploads the evidence even when verification fails.

Full runs each ignored GTK test by its exact discovered name, in a fresh Xvfb
display at 1600×1200 pixels and D-Bus session. The explicit screen size keeps
wide-window fixtures independent of distribution defaults. Separate processes are
necessary because GTK thread ownership cannot be reset merely by setting `--test-threads=1`. Native window renders emitted by
existing tests go to `screenshots/`; inspect them before declaring a visual change
complete. Ordinary tests alone are not the delivery gate.

Clippy diagnostics are compared with `scripts/clippy-baseline.json`. The list is currently empty; the legacy audit warning was fixed when the journal
writer changed. Entries identify a warning by code, file, line and message. A new or
moved warning fails. Review rather than regenerate the exception list; remove an
entry when the warning is fixed. Dependency checks inspect resolved package names,
including aliases and transitive normal/build dependencies. Source-level OS calls
and state/event correspondence still need review and behavioral tests.

## Test changed behavior

For behavior changes, follow the constitution's red-green-refactor cycle. Reuse an
existing fixture or test table when it covers the case. Put the regression at the
lowest layer that can reproduce the defect: pure core logic, a real-server IPC
test, or GTK when widget behavior requires it. Add another layer only for a
distinct contract or integration risk. Assert observable state and emitted events;
prefer a fixture's readiness signal to an arbitrary delay.

During iteration, run focused tests before the shared verification command:

```sh
cargo test -p signaltty-core <test-filter>
cargo test -p signaltty-server --test <integration-test-target> <test-filter>
```

These commands do not replace `scripts/verify.sh full`. Documentation-only changes
need no behavioral test. Verify their commands, paths and source ownership instead,
then run the same full delivery gate.

## Check affected paths

Before calling a behavior change complete, record which of these apply in the PR
evidence and verify those paths. An unsupported path needs an explicit reason.

- Entry points: GUI actions, keyboard shortcuts, context menus and CLI commands
  that invoke the same operation should observe the same server-owned result.
- Clients and contracts: trace the typed protocol, server handler, state transition
  and emitted event through affected GUI, CLI and agent/plugin consumers. Keep
  business decisions in the server/core so another client can use the operation.
- Lifecycle: verify the reverse transition where supported, plus failure,
  cancellation, disconnect/reconnect and restored state when relevant. Confirm
  that late replies cannot act on a closed or replaced pane.
- Providers: for agent behavior, inspect affected adapters and hooks. Use the
  existing synthetic provider fixtures where sufficient; record real-provider
  checks separately because synthetic proof does not establish provider behavior.
- Performance: for terminal, layout and refresh changes, check bounded output,
  refresh coalescing and terminal-widget retention using the existing feature map
  and benchmark. Measure suspected regressions before adding a workaround.
- Documentation: update the current architecture or workflow guide that owns the
  changed behavior. Research notes and old verification reports describe their
  dated checkout and are not current operating instructions.

## Native visual and accessibility acceptance

The automated matrix includes native light/dark renders, narrow diff/dialog cases,
keyboard navigation, focus restoration, stale responses and stable terminal widgets.
The [feature map](../.agents/skills/verify-signaltty/features/README.md) identifies
the test for each behavior. A reviewer checks:

- The pending approval and selected workspace are identifiable without relying on color alone.
- Text, controls and error messages remain readable in light/dark and narrow windows.
- Keyboard actions reach the intended control and restore focus after dismissal.
- Loading, empty, disconnected and failed states explain what happened and the available action.
- Long names and enlarged text remain usable without hiding required decisions.

Before changes affecting accessibility ship, explicitly test high contrast, large
text, screen-reader names/order and on-screen keyboard input as described by
[GNOME HIG](https://developer.gnome.org/hig/guidelines/accessibility.html).
Current CI does not certify these manual checks, IME, folder choosers, notification
clicks or paid provider sessions. Record `not run` rather than `passed` for them.

The existing `qa-ui-scenes.py` capture path requires KDE KWin/Spectacle. Use its
documented light/dark, `--width 360 --long-choice`, `--font 'Sans 18'` and
`ADW_DEBUG_HIGH_CONTRAST=1` scenes on a supported desktop. The desktop reliability
probe uses the session accessibility bus and must not run while the user's
signaltty GUI is open. A stale AT-SPI bus is an environment failure, not a passing
app check. A second reviewer evaluates substantial UI changes against this rubric;
human product judgment resolves subjective disagreements.

## Parallel work and handoff

Create one worktree and branch per independent task. Use
`SPECIFY_FEATURE_DIRECTORY=specs/<feature>` and `SPECIFY_FEATURE_NO_PERSIST=1`
when an orchestrator selects a feature explicitly. Each checkout's tracked
`.specify/feature.json` belongs to that checkout. Share contracts before splitting
work; assign one writer to a file and one owner to final integration.

Use helper-owned temporary sockets, state and plugin directories. GUI automation
also needs an isolated D-Bus session because the production application ID is
shared. Never stop processes by name or clean another task's directories.

Create a new handoff with `scripts/agent.sh handoff --feature <full-slug>`.
It gathers facts under ignored `target/handoffs/` and accepts `--notes` for
decisions/ownership. Use `--output` for a durable new path; an existing file is
never overwritten. Saving nonignored source after verification makes proof stale.
The decisions section should cover the following for multi-session work:

```text
Branch/worktree and base/current commit:
Accepted requirements and decisions:
Completed requirements, with verification command and artifact path:
Uncommitted changes and file ownership:
Remaining failures or unverified behavior:
Next concrete step:
```

Commit reproducible instructions and selected release evidence, not every transient
log. The runner's summary complements this handoff; it does not replace task status.

## Evaluate improvements to the agent workflow

Use `docs/qa/agent-evaluation/tasks.json` as a starting task bank. Each trial starts
at its recorded baseline in a disposable worktree. Keep model, tool versions,
resource limits and task prompt fixed when comparing two instruction/tooling setups.
Run multiple trials and vary order; a single successful run is not an improvement
estimate. Do not run trials in the user's active workspace.

Copy `result-template.json` per trial. Record the actual final commit and evidence,
success criteria, human interventions, regressions, setup/runtime, token/cost data
when available, and environment failures separately. `null` means not measured.
Compare completion and defects first; faster output with broken behavior is not a win.
Include workflow discovery, an interrupted handoff and recognition of stale evidence.
Compare one routing/instruction change at a time and retain traces to diagnose
wrong results or evaluator mistakes. The workflow commands make no measured
claim about agent productivity.
Have a reviewer verify outcomes independently of the implementing agent's summary.
This PR supplies the protocol and fixtures, not measured claims about agent quality.

The [research note](research/2026-10-03-agent-ready-repository.md) records sources
and limits. Add or remove skill requirements based on these trials and recurring
failures, then update the routing document and constitution when necessary.
