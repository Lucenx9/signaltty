# Automatic hook configuration verification — 2026-09-30

Scope: specs/010-automatic-agent-hooks, baseline main 375fc8f. Native Linux,
GTK4/libadwaita/VTE; no paid model requests or changes to real provider configs.

## Observed behavior

Real-PTY fake-provider tests load installed configuration at process startup,
execute the generated reporter commands through the shell, and observe actual
server Idle → Working → Blocked → Done states. Splits prepare before startup.
An official adapter resume after server restart recreates removed Codex hooks
inside its original custom config root. Fake API credentials are absent from
persisted state. CLI JSON keeps the setup report; human output shows the notice.

Malformed config and foreign plugins survive byte-for-byte; ordinary terminal
execution continues after setup failure. Shells, hints and generic `agent` do
not authorize provider writes. Relative roots resolve against the launch cwd;
Claude bare/excluded settings modes remain disabled. Codex hooks are configured
without trust-state edits or bypass flags.

Installer regressions cover mixed ownership, foreign metadata/empty events,
compound legacy commands/operators/newlines, refresh, concurrent installation,
quoted executable paths, idempotent no-op bytes/timestamps, permissions, symlinks,
relative config roots and FIFO rejection. Preservation regressions reproduced
red before their fixes.

## Review

Separate Standards and Spec reviews found and reproduced preservation, ownership,
nonregular-file, disabled-option syntax and relative-root problems. All were
corrected and regression-tested. Final reviews report no material blockers.

## Verification

- `cargo build --workspace`: passed.
- `cargo test --workspace`: passed: 206 tests, no failures.
- `cargo clippy --workspace --all-targets`: passed; only the two unchanged
  existing warnings at audit.rs:111 and router.rs:300.
- `cargo fmt --check`, `git diff --check`: passed.
- Eleven ignored GTK display tests: passed, each in its own process/session.
  GTK must initialize on one thread; one Rust test process per ignored test
  avoids test-harness thread affinity errors even with --test-threads=1.
- Native setup notice test confirms literal text in ForceLight and ForceDark.

To repeat display checks, enumerate `cargo test -p signaltty-gui -- --list --ignored`,
then run each name individually with
`dbus-run-session -- cargo test -p signaltty-gui -- <name> --exact --ignored --test-threads=1`.

Provider probes: Claude 2.1.285, Codex 0.159.0, OpenCode 1.18.33 installed.
[OpenCode v1.18.33 source](https://github.com/anomalyco/opencode/blob/v1.18.33/packages/opencode/src/plugin/index.ts#L99-L123) confirms identical export functions are deduplicated; its session schema supplies sessionID and status objects.
The local `agent` executable is Grok 1.0.44, so Cursor was not exercised against
a local runtime. Provider schema tests still cover Cursor installation. Tests
use fixtures and temporary provider roots, not live model sessions. Installing
configuration does not prove a provider trusted or loaded it: Codex still owns
its native `/hooks` review. Direct launch is the automatic trigger; agents typed
inside existing shells require previously configured hooks or manual installation.

Delivery: pending push and remote CI.
