# signaltty — native Linux workspace for parallel AI coding agents

A terminal multiplexer designed for agent-driven development: a persistent
session server owns PTYs and state; a CLI and a native GTK4/libadwaita GUI
attach as pure clients. Closing the UI never kills running agents.

**Status: Phase 4 (extensibility) implemented and tested.**
All MVP phases (multiplexer core, agent awareness, native GUI,
executable plugins) are done.

## Quick start

```bash
cargo build --workspace
./target/debug/signaltty daemon          # start the session server
./target/debug/signaltty integration install claude # optional: agents typed inside a shell
./target/debug/signaltty new --cwd ~/code/project -- codex
./target/debug/signaltty new --cwd ~/code/project -- claude
./target/debug/signaltty pane read <pane-id>
./target/debug/signaltty pane attach <pane-id>   # Ctrl+] to detach
./target/debug/signaltty wait --pane <id> --until done
./target/debug/signaltty pane resume <pane-id>   # after restart, via native resume
```

Note: codex trusts new hooks interactively ("Trust all and continue"
once per hook command); `codex exec` needs
`--dangerously-bypass-hook-trust` to fire untrusted hooks.

Global flags: `--socket <path>` (default
`$XDG_RUNTIME_DIR/signaltty/signaltty.sock`), `--json` (machine-readable
`result` object; resource-creating commands return opaque ids).

## Architecture

```text
signaltty-server ── owns PTYs, processes, workspaces, scrollback,
                    lifecycle, attention, persistence, IPC
signaltty ───────── CLI client (same API as the GUI)
signaltty-gui ───── GTK4/libadwaita GUI: sidebar, tabs, splits,
                    VTE panes, attention rings, desktop notifications
```

GUI system deps: `libgtk-4-dev libadwaita-1-dev
libvte-2.91-gtk4-dev` (libadwaita ≥ 1.7; GUI crate only; server/CLI
build without them).

## Install the desktop app

```bash
./contrib/install-desktop.sh install
```

This builds the GUI in release mode and installs `signaltty-gui` in
`~/.local/bin`, the desktop entry in `~/.local/share/applications`, the
AppStream metadata in `~/.local/share/metainfo`, and both icons in
`~/.local/share/icons/hicolor/scalable/apps`. The installed desktop entry
launches the binary by its absolute path, so it works even when `~/.local/bin`
is not on the desktop session's `PATH`. Sign out and back in if the launcher
does not refresh. The GUI connects to the signaltty server.

To remove these files:

```bash
./contrib/install-desktop.sh uninstall
```

To close a workspace, use the close button on its sidebar row, or select it
and choose **Close Workspace** from the main menu. Confirming stops its
running terminals and agents. Closing the GUI window leaves them running.

GUI shortcuts (Ctrl+Shift, so plain Ctrl chords reach the terminal):

| Keys | Action |
|---|---|
| Ctrl+Shift+N / Ctrl+Shift+T | new workspace (folder · name · agent) / new tab |
| Ctrl+Shift+E / Ctrl+Shift+O | split right / split down |
| Ctrl+Shift+W | close pane |
| Ctrl+Shift+J | jump to the next pane that needs you |
| Ctrl+Shift+P | command palette and workspace search |
| Ctrl+Shift+R | rename the selected workspace |
| Ctrl+Shift+F | search the active terminal, with next/previous matches |
| Ctrl+Shift+Z | zoom the active pane, then restore its splits |
| Alt+1…9, Ctrl+PgUp/PgDn | switch tabs |
| F9 / F10 | toggle sidebar / main menu |

The main menu also offers **Worktrees** and **Show Changes**. Worktrees lists
Git's registered checkouts and opens each as a workspace. Create a checkout
on a new branch to isolate another agent's edits. **Show Changes**
(Ctrl+Shift+D) docks a panel beside the terminals with working-tree changes
against HEAD, grouped by directory, with per-file counts and binary/untracked
labels. Pane zoom changes only the view; saved divider ratios stay intact.

The same checkout operations are available to scripts:

```bash
signaltty worktree list --workspace my-project
signaltty worktree create --workspace my-project --path /absolute/feature --branch feature
signaltty worktree open --workspace my-project --path /absolute/feature
signaltty workspace close feature
signaltty worktree remove --workspace my-project --path /absolute/feature
```

Closing a workspace preserves its checkout. Removal refuses the main checkout,
dirty/locked checkouts and open workspace or live-process references; branches
are kept. There is no force-removal fallback.

Claude and current Codex permission hooks show **Allow once** and **Deny** in
the existing pane bar. The choice returns as native hook JSON. Timeout or a
cancelled reporter grants nothing and leaves permission handling to the agent.
Codex still requires its normal hook trust review. See [agent integration](docs/07-agents.md#native-permission-replies-adr-0015).

Server → Workspace → Tab → Pane. Lifecycle (unknown/working/blocked/
done/idle/failed/exited) and attention (none/unread/input_required/
permission_required/warning/error) are independent first-class axes.

Docs: [architecture](docs/01-architecture.md) · [data model](docs/02-data-model.md) ·
[lifecycle & attention](docs/03-lifecycle-attention.md) · [PTY](docs/04-pty-lifecycle.md) ·
[terminal research](docs/05-terminal-backend.md) · [GTK decision](docs/06-gui-toolkit.md) ·
[agents](docs/07-agents.md) · [IPC schema](docs/08-ipc.md) · [persistence](docs/09-persistence.md) ·
[security](docs/10-security.md) · [crates](docs/11-crates.md) · [roadmap & risks](docs/12-roadmap-risks.md) ·
[plugins](docs/13-plugins.md) · [ADRs](docs/adr/).

Plugins live in `~/.config/signaltty/plugins/<name>/plugin.toml`
(event hooks + runnable commands); examples in `examples/plugins/`,
orchestration recipe in `examples/workflows/`.

## Layout

```text
crates/signaltty-core    domain model (toolkit/async/OS-free)
crates/signaltty-proto   signaltty/1 IPC envelopes, methods, events
crates/signaltty-term    TerminalBackend trait, headless vt100 backend,
                         OSC 9/99/777 scanner, sanitizers
crates/signaltty-agent   AgentAdapter trait + codex/claude/opencode/
                         cursor/generic adapters, hook classification,
                         session identity, resume commands
crates/signaltty-server  daemon: PTY manager, router, events, snapshots
crates/signaltty-cli     CLI client incl. interactive attach
crates/signaltty-gui     native GUI (sidebar/tabs/splits/VTE,
                         attention rings, next-unread, notifications)
crates/signaltty-testkit hermetic-server test harness (real PTYs)
contrib/                 systemd user unit and desktop integration
```

## Run as a user service (recommended)

```bash
mkdir -p ~/.config/systemd/user
cp contrib/signaltty-server.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now signaltty-server
```

## Development

```bash
scripts/setup-dev.sh --system # Ubuntu 26.04; installs packages and pinned Rust components
scripts/verify.sh doctor
scripts/verify.sh full
```

The development container uses the same setup. Other Linux distributions can
install equivalents of `scripts/dev-packages.txt`, then run `scripts/setup-dev.sh`.
See [agent development](docs/17-agent-development.md) for checks, evidence,
worktree isolation and native desktop verification limits.

## Design rule

The server owns the processes, the terminal backend renders terminal
state, agent adapters provide semantic agent state, notifications
describe human attention, and the UI helps the user find the work
that needs them.

Direct Claude, Codex, OpenCode and Cursor launches configure their status hooks
automatically, preserving existing configuration. Codex may still require native
`/hooks` trust review. Setup problems appear as notices while the terminal remains
usable. See [agent integration](docs/07-agents.md#automatic-configuration-adr-0013).
