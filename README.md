# signaltty — native Linux workspace for parallel AI coding agents

A terminal multiplexer designed for agent-driven development: a persistent
session server owns PTYs and state; a CLI and a native GTK4/libadwaita GUI
attach as pure clients. Closing the UI never kills running agents.

**Status: Phase 4 (extensibility) implemented and tested.**
All MVP phases (multiplexer core, agent awareness, native GUI,
executable plugins) are done.

## Quick start

```bash
cargo build --release
./target/debug/signaltty daemon          # start the session server
./target/debug/signaltty integration install all   # hook shims (codex/claude/opencode/cursor)
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

GUI shortcuts (Ctrl+Shift, so plain Ctrl chords reach the terminal):

| Keys | Action |
|---|---|
| Ctrl+Shift+N / Ctrl+Shift+T | new workspace (folder · name · agent) / new tab |
| Ctrl+Shift+E / Ctrl+Shift+O | split right / split down |
| Ctrl+Shift+W | close pane |
| Ctrl+Shift+J | jump to the next pane that needs you |
| Alt+1…9, Ctrl+PgUp/PgDn | switch tabs |
| F9 / F10 | toggle sidebar / main menu |

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
contrib/                 systemd user unit
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
cargo build --workspace
cargo test --workspace        # 60+ tests: unit + real-PTY integration + CLI e2e
cargo clippy --workspace --all-targets
```

## Design rule

The server owns the processes, the terminal backend renders terminal
state, agent adapters provide semantic agent state, notifications
describe human attention, and the UI helps the user find the work
that needs them.
