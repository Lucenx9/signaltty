---
name: verify-signaltty
description: Verify signaltty's native GTK workspace, real PTYs and IPC reliability with isolated server and graphical QA harnesses.
---

## Launch

Use `scripts/verify.sh doctor` for prerequisites, `scripts/verify.sh fast` during
development, and `scripts/verify.sh full` before submitting a PR. Full includes
every ignored GTK test in a separate Xvfb/D-Bus process, server QA and the refresh
benchmark. Each run prints a unique evidence directory with `summary.json`, logs
and available native renders. Inspect those renders; a green run is not visual approval.
`scripts/verify.sh desktop` additionally runs the native AT-SPI reliability probe.
See [development and proof limits](../../../docs/17-agent-development.md).

Run `cargo build --workspace`. The helpers below launch their own server using
temporary socket/state/plugin/agent directories, wait for `server.status`, and
clean up in `finally`. Do not launch a second GUI while a user's signaltty window
is open: its application ID is shared and activation presents that window.

## Doctor

Run `test -x target/debug/signaltty-server && test -x target/debug/signaltty-gui`.
For GUI checks require a real desktop, Python `gi` with Gio/GLib/Atspi, and no
existing signaltty GUI. Inspect the helper's owned PID and logs if a probe fails.
A stale accessibility bus can make observations fail independently of the app;
finish isolated display tests before starting native GUI probes.

## Drive

- `python3 scripts/qa-server-edge-cases.py` tests real IPC state invariants.
- `python3 scripts/qa-gui-reliability.py` invokes production GTK actions and
  reads VTE/labels through AT-SPI, with a socket relay for disconnects.
- `ADW_DEBUG_COLOR_SCHEME=prefer-light python3 scripts/qa-gui-reliability.py --hold-seconds 60`
  leaves the owned window open briefly for light-theme capture. On KDE add
  `--screenshot /tmp/signaltty-light.png` to capture before teardown.
- `python3 scripts/bench-gui-refresh.py --check` verifies refresh coalescing.
- `ADW_DEBUG_COLOR_SCHEME=prefer-light python3 scripts/qa-ui-scenes.py --output /tmp/signaltty-ui.png`
  captures a populated isolated window through KDE KWin/Spectacle. Repeat with
  `prefer-dark`, `ADW_DEBUG_HIGH_CONTRAST=1`, `--width 360 --long-choice`, or
  `--font 'Sans 18'`. The font override uses temporary GTK settings and disables
  portal font overrides; it never changes desktop preferences. It checks
  the owned window's active state and actual width before capture.

Use the [feature map](features/README.md) for what each probe proves. GTK ignored
tests run individually with `GTK_A11Y=none dbus-run-session -- cargo test -p
signaltty-gui <filter> -- --ignored --test-threads=1`.

## Evidence

The runner saves logs under `target/verification/`; CI uploads its evidence artifact.
For durable release evidence, save selected helper stdout JSONL under `docs/qa/`
with the verification date and commit.
Proof includes the action and actual resulting state: decisions remain answerable,
VTE contains outage and online markers exactly twice for the echoing cat fixture,
GTK responds during SIGSTOP, and server ownership remains valid. Synthetic hooks
exercise the production agent boundary without paid provider requests.
Capture only the helper's still-live window; check its PID before capturing and
inspect the image before keeping it. Never substitute an unrelated active window.

## Cleanup

Helpers stop only processes they created, resume a paused server before shutdown,
terminate their GUI and relay, then remove temporary state. Keep JSONL and verified
screenshots outside the temporary directory. Never kill by process name or use the
default server socket. After a failed probe verify no owned processes remain.

## Helpers

The two QA scripts are executable repository helpers; the benchmark is invoked with Python. Their exit status must
be zero; a screenshot alone cannot replace a failed assertion. Keep maps current
with `/maintain-verification-skill` when user-visible paths change.
