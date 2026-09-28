# 08 — IPC Schema (`signaltty/1`)

Transport: Unix domain socket at
`$XDG_RUNTIME_DIR/signaltty/signaltty.sock` (mode 0700 dir, 0600
socket, owned by user). Framing: newline-delimited JSON (JSONL), one
request per line, one response per line on the same connection;
server→client events are JSONL messages with `type: "event"`.
Binary PTY data inside events is base64. Max line 16 MiB (PTY bursts
are chunked).

## Envelope

Request:

```json
{"protocol":"signaltty/1","id":"c1","method":"pane.spawn","params":{…}}
```

Response (ok):

```json
{"protocol":"signaltty/1","id":"c1","ok":true,"result":{…}}
```

Response (error):

```json
{"protocol":"signaltty/1","id":"c1","ok":false,"error":{"code":"NO_SUCH_PANE","message":"…","details":{}}}
```

Event:

```json
{"protocol":"signaltty/1","type":"event","event":"agent.done","seq":412,"payload":{"pane_id":"pane_…"}}
```

Rules: `protocol` mismatch → `BAD_PROTOCOL` error, connection stays
open. Unknown method → `UNKNOWN_METHOD`. Unknown fields ignored
(forward compat). Every mutating result returns opaque ids. `seq` is a
monotonic server counter; clients can `subscribe {from_seq}` to replay.

## Methods

| Method | Params | Result |
|---|---|---|
| `server.status` | — | `{version, protocol, uptime_s, workspaces, tabs, panes, live_panes}` |
| `server.shutdown` | `{force?}` | `{stopped}` (refuses with `PANES_ALIVE` unless force) |
| `workspace.create` | `{name?, cwd?}` | `{workspace}` |
| `workspace.list` | — | `{workspaces:[…]}` |
| `workspace.get` | `{workspace_id}` | `{workspace, tabs:[…]}` |
| `workspace.rename` | `{workspace_id, name}` | `{workspace}` |
| `workspace.close` | `{workspace_id, signal?}` | `{closed}` |
| `workspace.refresh_git` | `{workspace_id}` | `{workspace}` |
| `tab.create` | `{workspace_id, title?}` | `{tab}` |
| `tab.close` | `{tab_id}` | `{closed}` |
| `tab.set_layout` | `{tab_id, layout}` | `{tab}` |
| `pane.spawn` | `{workspace_id, tab_id?, cwd?, argv, env?, cols?, rows?, agent_hint?}` | `{pane}` |
| `pane.split` | `{pane_id, direction: "right"\|"down", argv?, cwd?}` | `{pane}` (new sibling) |
| `pane.get` | `{pane_id}` | `{pane}` |
| `pane.input` | `{pane_id, data_b64}` | `{written}` |
| `pane.resize` | `{pane_id, cols, rows}` | `{pane}` |
| `pane.signal` | `{pane_id, signal, group?}` | `{sent}` |
| `pane.read` | `{pane_id, mode: "screen"\|"tail", lines?, strip_ansi?}` | `{text, truncated}` |
| `pane.attach` | `{pane_id, cols?, rows?, mark_seen?}` | `{snapshot_b64, size, live, ...}` then `pty.data` stream; the snapshot is replayable VT state (contents, colours, cursor, modes) (`mark_seen` default true; GUIs pass false and clear on focus) |
| `pane.detach` | `{pane_id}` | `{detached}` (also implicit on disconnect) |
| `pane.close` | `{pane_id, signal?}` | `{closed}` |
| `pane.resume` | `{pane_id}` | `{pane}` (spawns adapter resume argv; errors unless restored+resumable) |
| `pane.mark_seen` | `{pane_id}` | `{pane}` (clears attention) |
| `notify` | `{pane_id?, title, body?, severity?}` | `{notification}` |
| `hook-event` | `{agent, event, pane_id?, client_pid?, payload?, message?, title?, severity?}` | `{accepted, agent, event, pane_id, lifecycle?, attention?}` (adapter classification; pane by explicit id or `client_pid` ancestry) |
| `report-session` | `{pane_id, agent_session_id, agent?}` | `{pane}` |
| `subscribe` | `{events?: ["agent.*","attention.*",…], from_seq?}` | `{subscribed, seq}` then event stream |
| `wait` | `{pane_id, until, timeout_s?}` | `{satisfied, state}` or `TIMEOUT` |
| `focus.next_unread` | — | `{pane_id?}` (severity→recency order) |
| `plugin.list` | — | `{dir, plugins[], failures[]}` (hooks with runs/errors/last_error; see 13) |
| `plugin.reload` | — | same as `plugin.list` after re-scan (stats reset) |

`until`: `blocked | done | idle | failed | exited | seen | attention_cleared`.
Glob subscriptions: `*`, `agent.*`, `pane.*`, `workspace.*`, `tab.*`,
`attention.*`, `notification.*`, `git.*`, `pty.*`.

## Events (all carry `seq`)

```text
workspace.created  workspace.updated  workspace.closed
tab.created        tab.updated        tab.closed
pane.created       pane.updated       pane.exited        pane.closed
pane.resized       pty.data           pty.snapshot
agent.working      agent.blocked      agent.done         agent.failed
agent.idle         agent.unknown
attention.created  attention.updated  attention.cleared
notification.created
git.branch_changed
server.will_shutdown
```

`pty.data {pane_id, data_b64}` streams only to connections that ran
`pane.attach` for that pane. All other events go to `subscribe`rs by
glob match.

## Error codes

`BAD_PROTOCOL, UNKNOWN_METHOD, BAD_PARAMS, NO_SUCH_{SERVER,WORKSPACE,TAB,PANE},
PANE_EXITED, PANES_ALIVE, SPAWN_FAILED, IO_ERROR, TIMEOUT, RATE_LIMITED,
FORBIDDEN, INTERNAL`.

## CLI mapping

Every CLI command is a thin wrapper over one method call with `--json`
passthrough of `result`. Examples: `signaltty new` → `workspace.create`
+ `pane.spawn`; `signaltty pane read` → `pane.read`; `signaltty wait`
→ `wait`; `signaltty notify` → `notify`. The CLI injects
`$SIGNALTTY_PANE` context for `notify`/`hook-event` when run inside a
pane (env set by the server at spawn).
