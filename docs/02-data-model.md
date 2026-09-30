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

Git worktree registrations are queried from Git rather than copied into the
snapshot. A workspace's canonical `cwd` associates it with a checkout;
`worktree.list` reports the open workspace IDs for each registered checkout.
Create and open produce ordinary workspaces with stable handles. Closing a
workspace preserves its checkout and branch. Removal is a separate operation
and refuses the main checkout, dirty or locked checkouts, and open references.

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

The GUI renders this tree; the server validates and stores it. Each tab
layout must contain every pane owned by that tab exactly once. Whole-layout
updates arrange panes; deleting a pane requires `pane.close`. Ratios must be
finite and normalize to 0.05–0.95, matching individual divider updates.
Pane and tab workspace ownership must agree. Closing a tab or workspace
cleans every pane it owns, including panes hidden by legacy layouts.
Automatic tabs become visible only after their first process spawns.

Splitting a pane
replaces a `Pane` leaf with a `Split`, then rebalances the run: every
visual sibling in the same direction gets an equal share, so repeated
splits divide a tab into thirds, quarters, and so on instead of
halving the newest pane. Closing the last pane of a tab
closes the tab (configurable: keep empty tabs or not; default close).
Divider positions are plain ratios in the tree; clients move one
divider with `tab.set_ratio` (path of first/second choices from the
root, ratio clamped to 0.05–0.95), which persists like any other tab
change.

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
| `lifecycle_since` | when `lifecycle` last changed (optional; absent in old snapshots) |
| `last_run_secs` | length of the latest `working` stretch — GUI renders "Working for 2m…" → "Worked for 2m" |
| `last_message` | latest explicit notification / hook payload summary (never raw scrollback scrape as primary) |
| `pending_decision` | at most one structured decision `{id, prompt, options[{id, label}], answerable, received_at}` set by `hook-event` (directive 2); rendered as inline buttons iff `answerable`, cleared on answer / attention-clear / leaving `blocked` / exit |
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

Native PermissionRequest decisions have a transient server response route to
their waiting reporter. The public decision is render data; its ID and live
route jointly identify the request that an answer can resolve. Route loss,
timeout, supersession, pane exit or session cancellation cannot grant access.
A saved decision cannot restore the reporter connection or its answerability.

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

`AgentInfo.config_env` retains only explicit `CLAUDE_CONFIG_DIR`, `CODEX_HOME`
and `OPENCODE_CONFIG_DIR` paths, normalized against launch cwd, for official
resume. Other environment variables and credentials are not retained.
