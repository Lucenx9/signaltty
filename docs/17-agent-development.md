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
is required by the verification runner. The locked GTK packages require Rust 1.92;
development uses 1.94.0 and CI compiles all targets separately on 1.92.0.

To build the same container without an editor:

```sh
docker build -f .devcontainer/Dockerfile -t signaltty-dev .
docker run --rm -v "$PWD:/workspace" -w /workspace signaltty-dev scripts/verify.sh doctor
```

The image runs as root; use a disposable checkout or a named workspace volume for
builds to avoid changing ownership of host build artifacts. The Ubuntu tag and apt
packages receive updates; this setup is repeatable, not bit-for-bit reproducible.

## Choose the verification scope

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
Logs are saved before running the next step. On failure, read the failed step log
and `qa-logs/`. CI uploads the evidence even when verification fails.

Full runs each ignored GTK test by its exact discovered name, in a fresh Xvfb
display and D-Bus session. This is necessary because GTK thread ownership cannot
be reset merely by setting `--test-threads=1`. Native window renders emitted by
existing tests go to `screenshots/`; inspect them before declaring a visual change
complete. Ordinary tests alone are not the delivery gate.

Clippy diagnostics are compared with `scripts/clippy-baseline.json`. The exception
identifies the existing audit.rs warning by code, file, line and message. A new or
moved warning fails. Review rather than regenerate the exception list; remove an
entry when the warning is fixed. Dependency checks inspect resolved package names,
including aliases and transitive normal/build dependencies. Source-level OS calls
and state/event correspondence still need review and behavioral tests.

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

Keep the following handoff in the feature directory for interrupted or multi-session work:

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
Have a reviewer verify outcomes independently of the implementing agent's summary.
This PR supplies the protocol and fixtures, not measured claims about agent quality.

The [research note](research/2026-10-03-agent-ready-repository.md) records sources
and limits. Add or remove skill requirements based on these trials and recurring
failures, then update the routing document and constitution when necessary.
