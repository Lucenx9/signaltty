# 01 — Architecture & Process Model

## Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                    signaltty session server                   │
│                    (one process, per user)                    │
│                                                               │
│  ┌────────────┐  ┌──────────────┐  ┌───────────────────────┐  │
│  │ Workspace  │  │ PTY manager  │  │ Agent state engine    │  │
│  │ store      │  │ (owns PTYs,  │  │ (adapters, lifecycle, │  │
│  │ (tabs,     │  │  procs,      │  │  attention, notifs)   │  │
│  │  panes,    │  │  scrollback) │  │                       │  │
│  │  layouts)  │  │              │  │                       │  │
│  └────────────┘  └──────────────┘  └───────────────────────┘  │
│  ┌────────────┐  ┌──────────────┐  ┌───────────────────────┐  │
│  │ Git meta   │  │ Persistence  │  │ IPC router            │  │
│  │ provider   │  │ (snapshot +  │  │ (Unix socket, pub/sub │  │
│  │ (cached)   │  │  restore)    │  │  events, wait)        │  │
│  └────────────┘  └──────────────┘  └───────────────────────┘  │
└──────────────┬───────────────────────────────┬───────────────┘
               │ Unix socket                   │ Unix socket
               │ $XDG_RUNTIME_DIR/signaltty/   │ (same API)
               ▼                               ▼
┌──────────────────────────────┐  ┌───────────────────────────┐
│ Native GUI (GTK4/libadwaita) │  │ CLI (`signaltty`)         │
│ - sidebar, splits, rings     │  │ - workspaces/tabs/panes   │
│ - terminal widgets (VTE)     │  │ - notify, wait, status    │
│ - desktop notifications      │  │ - JSON output, automation │
└──────────────────────────────┘  └───────────────────────────┘
               ▲                               ▲
               │ same socket API               │ same socket API
┌──────────────┴───────────────┐  ┌────────────┴──────────────┐
│ Agent hooks / plugins        │  │ Future: GUI-less TUI,     │
│ (Claude/Codex/OpenCode/      │  │ remote bridge, plugins    │
│  Cursor shims calling CLI)   │  │ (subprocess, via CLI/API) │
└──────────────────────────────┘  └───────────────────────────┘
```

## Process model

- **Session server** (`signaltty-server`): exactly one per user by
  default (socket path is the singleton lock). Owns all PTYs, child
  processes, scrollback, workspace/tab/pane state, agent lifecycle,
  attention, persistence, IPC. No GUI code, no rendering. Must run
  headless (systemd user service or `signaltty daemon`).
- **GUI** (`signaltty-gui`, Phase 3): a pure client. Renders terminal
  state streamed from the server, sends input, subscribes to events.
  Closing or crashing it never touches PTYs or child processes.
- **CLI** (`signaltty`): a pure client of the same API. Used by humans
  and by agent hooks/plugins for `notify`, `report-agent-session`,
  `wait`, `read`, `send-input`.
- **Agent CLIs** (codex, claude, opencode, cursor-agent, …): unmodified
  third-party binaries running as the foreground process of a pane's
  PTY. Zero wrapping of their workflows; integration happens via
  hooks/events/OSC/process info around them, never by reimplementing
  them.

## Client/server rules

1. All durable or process-owning state lives in the server.
2. Clients hold no authoritative state; on connect they fetch a
   snapshot then subscribe to events.
3. Every resource has an opaque stable ID (UUIDv4 string). Clients
   never derive identity from labels or indexes.
4. Multiple clients may attach simultaneously (GUI + CLI, two GUIs).
   Terminal size arbitration is defined in
   [04](04-pty-lifecycle.md); last-writer-wins for resizes, with the
   server clamping to sane bounds.
5. The protocol is versioned (`signaltty/1`); unknown
   request/event fields are ignored for forward compatibility.

## Event flow (attention path)

```text
agent emits signal ──► server classifies ──► state updated ──► event fanned out
 (hook POST / OSC /     (adapter layer,        (lifecycle +        (GUI ring +
  CLI notify /           explicit beats          attention are       sidebar +
  exit / BEL)             heuristic)             independent)        desktop notif)
```
