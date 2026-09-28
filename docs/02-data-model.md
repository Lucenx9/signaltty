# 02 — Data Model (Server / Workspace / Tab / Pane)

```text
Server
└── Workspace "signaltty"  (repo / project / task)
    ├── Tab "agents"       (one layout)
    │   ├── Pane p1 (codex,  ~/code/foo)
    │   └── Pane p2 (claude, ~/code/foo)
    └── Tab "tests"        (another layout)
        └── Pane p3 (shell, ~/code/foo)
```

## Server

- `id`: opaque server id (stable per socket path).
- `socket_path`: `$XDG_RUNTIME_DIR/signaltty/signaltty.sock`.
- `state_dir`: `$XDG_STATE_HOME/signaltty` (`~/.local/state/signaltty`).
- Owns: workspaces, PTYs, event bus, persistence.

## Workspace

Usually one repo/project/task/investigation.

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `ws_…` |
| `name` | human label, editable, non-unique |
| `cwd` | default working directory for new panes |
| `repo_root`, `repo_name`, `branch`, `dirty` | cached Git context (see below) |
| `tabs` | ordered list of tab ids |
| `active_tab_id` | last focused tab (per last client; server keeps one canonical value) |
| `created_at`, `updated_at` | timestamps |

Git context is a lightweight cached enrichment, refreshed on
`pane.cwd` change, explicit `refresh`, and cheap filesystem events —
never a Git client, never aggressive polling.

## Tab

One named layout inside a workspace (`agents`, `shell`, `tests`, `logs`).

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `tab_…` |
| `workspace_id` | parent |
| `title` | human label |
| `layout` | binary split tree (see below) |
| `active_pane_id` | last focused pane in this tab |
| `created_at` | timestamp |

Layout is a binary tree:

```rust
enum Layout { Pane(PaneId), Split { dir: H|V, ratio: f32, first: Box<Layout>, second: Box<Layout> } }
```

The GUI renders this tree; the server only stores it. Splitting a pane
replaces a `Pane` leaf with a `Split`. Closing the last pane of a tab
closes the tab (configurable: keep empty tabs or not; default close).

## Pane

One terminal/PTY + process. Survives client detach while the server lives.

| Field | Meaning |
|---|---|
| `id` | opaque, e.g. `pane_…` |
| `workspace_id`, `tab_id` | parents (pane can be moved) |
| `title` | from `OSC 0/1/2` title sequences, else `argv[0]` |
| `cwd` | launch cwd; refreshed from `/proc/<pid>/cwd` on Linux |
| `argv` | launch command (persisted; see restore policy in [09](09-persistence.md)) |
| `pty_size` | `{cols, rows}` last arbitrated size |
| `live` | `Live | Exited {code, at}` |
| `agent` | `AgentInfo` (below), all optional for plain shells |
| `lifecycle` / `attention` | see [03](03-lifecycle-attention.md) |
| `last_message` | latest explicit notification / hook payload summary (never raw scrollback scrape as primary) |
| `scrollback_len` | bytes/lines retained (bounded ring) |
| `created_at`, `last_activity_at`, `last_seen_at` | timestamps |

## AgentInfo (per pane, adapter-owned)

| Field | Meaning |
|---|---|
| `kind` | `codex | claude | opencode | cursor | generic | none` |
| `agent_session_id` | native session id reported by adapter/hook (persisted) |
| `resume_argv` | adapter-built official resume command (persisted, never auto-run without consent) |
| `model`, `extra` | small adapter metadata map |

`agent_session_id` must come from hooks/APIs/env (`CLAUDE_CODE_SESSION_ID`,
`CODEX_THREAD_ID`, hook JSON payloads, OpenCode plugin events, Cursor
`hooks.json`), never from scraping terminal text when a better channel
exists.

## Notification

| Field | Meaning |
|---|---|
| `id` | opaque |
| `pane_id`, `workspace_id` | routing; pane may be unknown for server-level notes |
| `title`, `body` | sanitized plain text (control chars stripped) |
| `severity` | `info | warning | error` |
| `source` | `osc9 | osc99 | osc777 | cli | hook:<agent> | adapter | system` |
| `created_at`, `read_at` | timestamps |

## IDs

UUIDv4 strings with a short type prefix for human greppability
(`ws_`, `tab_`, `pane_`, `notif_`). Prefix is cosmetic; treat as opaque.
