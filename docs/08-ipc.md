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
(forward compat); present-but-mistyped fields → `BAD_PARAMS` (missing
and explicit null still mean "absent" for optionals). Every mutating
result returns opaque ids. `seq` is a monotonic server counter;
clients can `subscribe {from_seq}` to replay.

## Methods

| Method | Params | Result |
|---|---|---|
| `server.status` | — | `{version, protocol, uptime_s, workspaces, tabs, panes, live_panes}` |
| `server.shutdown` | `{force?}` | `{stopped}` (refuses with `PANES_ALIVE` unless force) |
| `server.schema` | — | `{protocol, version, methods[], events[], codes[]}` (self-printing contract, same constants the router matches on) |
| `workspace.create` | `{name?, cwd?}` | `{workspace}` |
| `workspace.list` | — | `{workspaces:[…]}` |
| `workspace.get` | `{workspace_id}` | `{workspace, tabs:[…]}` |
| `workspace.rename` | `{workspace_id, name}` | `{workspace}` |
| `workspace.close` | `{workspace_id, signal?}` | `{closed}` |
| `workspace.refresh_git` | `{workspace_id}` | `{workspace}` |
| `workspace.diff` | `{workspace_id}` | `{workspace_id, branch?, files[{path, added, removed, untracked, binary}], dirs[{dir, added, removed}], added, removed}` (worktree-vs-HEAD `git diff --numstat` as data; non-repo → `BAD_PARAMS`) |
| `tab.create` | `{workspace_id, title?}` | `{tab}` |
| `tab.close` | `{tab_id}` | `{closed}` |
| `tab.set_layout` | `{tab_id, layout}` | `{tab}` |
| `tab.set_ratio` | `{tab_id, path, ratio}` | `{tab}` |
| `pane.spawn` | `{workspace_id, tab_id?, cwd?, argv, env?, cols?, rows?, agent_hint?}` | `{pane, integration?}` |
| `pane.split` | `{pane_id, direction: "right"\|"down", argv?, cwd?}` | `{pane, integration?}` (new sibling) |
| `pane.get` | `{pane_id}` | `{pane}` |
| `pane.input` | `{pane_id, data_b64}` | `{written}` |
| `pane.resize` | `{pane_id, cols, rows}` | `{pane}` |
| `pane.signal` | `{pane_id, signal, group?}` | `{sent}` |
| `pane.read` | `{pane_id, mode: "screen"\|"tail", lines?, strip_ansi?}` | `{text, truncated}` |
| `pane.attach` | `{pane_id, cols?, rows?, mark_seen?}` | `{snapshot_b64, output_offset, size, live, ...}` then `pty.data` stream; the snapshot is replayable VT state (contents, colours, cursor, modes) (`mark_seen` default true; GUIs pass false and acknowledge on focus) |
| `pane.detach` | `{pane_id}` | `{detached}` (also implicit on disconnect) |
| `pane.close` | `{pane_id, signal?}` | `{closed}` |
| `pane.resume` | `{pane_id}` | `{pane, integration?}` (spawns adapter resume argv; errors unless restored+resumable) |
| `pane.mark_seen` | `{pane_id}` | `{pane}` (records reading; clears ordinary attention while preserving unanswered decisions and their required attention) |
| `decision.answer` | `{pane_id, decision_id, option_id}` | `{answered, lifecycle?, attention?}` (delivers through the pane adapter's channel and consumes the id; stale/consumed ids → `NO_SUCH_DECISION`, unknown option or channelless adapter → `BAD_PARAMS`) |
| `notify` | `{pane_id?, title, body?, severity?}` | `{notification}` |
| `hook-event` | `{agent, event, pane_id?, client_pid?, payload?, message?, title?, severity?, decision?}` | `{accepted, agent, event, pane_id, lifecycle?, attention?}` (adapter classification; pane by explicit id or `client_pid` ancestry; `decision: {id, prompt, options[{id, label}]}` sets/supersedes the pane's pending decision, captured `answerable` iff the adapter has a channel) |
| `report-session` | `{pane_id, agent_session_id, agent?}` | `{pane}` |
| `subscribe` | `{events?: ["agent.*","attention.*",…], from_seq?}` | `{subscribed, seq}` then event stream |
| `wait` | `{pane_id, until, timeout_s?}` | `{satisfied, state}` or `TIMEOUT` |
| `focus.next_unread` | — | `{pane_id?}` (severity→recency order) |
| `plugin.list` | — | `{dir, plugins[], failures[]}` (hooks with runs/errors/last_error; see 13) |
| `plugin.reload` | — | same as `plugin.list` after re-scan (stats reset) |

`until`: `blocked | done | idle | failed | exited | seen | attention_cleared`.
Glob subscriptions: `*`, `agent.*`, `pane.*`, `workspace.*`, `tab.*`,
`attention.*`, `notification.*`, `decision.*`, `git.*`, `pty.*`.

Every `workspace_id` param accepts a workspace id **or its handle**: ids
match first, then handles (namespaces are disjoint — handles never contain
`_`). Handles are immutable slugs derived at create (`My API!!` →
`my-api`, collisions → `my-api-2`); renames change only the name.

Every broadcast event except `pty.data` appends to
`$XDG_STATE_HOME/signaltty/audit.jsonl` (`{seq, event, payload, at}`;
rotation keeps one `.1` predecessor, 8 MiB cap each). `subscribe
{from_seq}` backfills from the file when the in-memory ring (1024) has
rotated or the server restarted — replays merge file + ring deduped by
`seq` (cap 4096).

`pane.spawn`, `pane.split` and `pane.resume` include an optional
`integration {agent, status, changed, file?, notice?}` for direct supported-agent
launches. Status is `configured`, `disabled`, or `error`. Configuration precedes
process execution; a setup error does not prevent launch. Clients show `notice`
when present. `configured` is not proof of provider trust or event delivery.
Ordinary shell/arbitrary launches have no integration result.

`pane.spawn` rejects an explicit tab from another workspace with `BAD_PARAMS`.
A failed process launch leaves no automatic tab or pane. Ownership validation,
process launch and pane publication complete before a concurrent close can
remove the target tab or workspace.

`tab.set_layout` requires every pane owned by the tab exactly once. Missing,
foreign or repeated pane IDs return `BAD_PARAMS` without mutation. Split ratios
must be finite and clamp to 0.05–0.95. Layout replacement arranges existing
panes; `pane.close` deletes one.

`tab.set_ratio` moves one divider: `path` holds 0 (first) / 1
(second) choices from the tab root (`[]` = root) and must resolve to
a `Split`, else `BAD_PARAMS`. `ratio` is clamped to 0.05–0.95 so a
client can never collapse a pane. The echo is `tab.updated`, like
`tab.set_layout`. GUIs persist dragged dividers through this method
rather than `tab.set_layout`: a targeted update cannot overwrite a
layout another client changed concurrently.

## Events (all carry `seq`)

```text
workspace.created  workspace.updated  workspace.closed
tab.created        tab.updated        tab.closed
pane.created       pane.updated       pane.exited        pane.closed
pane.resized       pty.data           pty.snapshot
agent.working      agent.blocked      agent.done         agent.failed
agent.idle         agent.unknown      agent.exited
attention.created  attention.updated  attention.cleared
decision.created   decision.answered  decision.cleared
notification.created
git.branch_changed
server.will_shutdown
```

`pty.data {pane_id, data_b64, output_offset}` streams only to connections
that ran `pane.attach` for that pane. `output_offset` is the cumulative end
byte offset of this output chunk. The attach reply carries the offset covered
by its snapshot, acquired atomically with terminal state. Streaming registration
precedes the final snapshot, and its response precedes subsequent stream events.
Clients discard or trim output already covered by that snapshot. Offsets are
per-pane for the lifetime of the server and are not persisted. Repeated attach
on one connection registers one viewer; detach removes it and preserves the
process. Viewer registration survives a child exit and `pane.resume`, so
existing connections receive resumed output without attaching again.
All other events go to `subscribe`rs by glob match.

## Error codes

`BAD_PROTOCOL, UNKNOWN_METHOD, BAD_PARAMS, NO_SUCH_{SERVER,WORKSPACE,TAB,PANE},
NO_SUCH_DECISION, PANE_EXITED, PANES_ALIVE, SPAWN_FAILED, IO_ERROR, TIMEOUT,
RATE_LIMITED, FORBIDDEN, INTERNAL`.

`decision.created {pane_id, decision, prev?}` fires on set (a supersede folds
into one emit carrying the previous id). `decision.answered
{pane_id, decision_id, option_id}` fires on delivery. `decision.cleared
{pane_id, decision_id, reason}` fires when the bar drops unanswered
(`reason`: `attention_cleared | moved_on | pane_exited`). A pending decision
clears on answer, when an agent transition clears its required attention or leaves `blocked`, or when the child exits. Reading,
focusing, `pane.mark_seen`, and default attach preserve the decision and gate.

## CLI mapping

Every CLI command is a thin wrapper over one method call with `--json`
passthrough of `result`. Examples: `signaltty new` → `workspace.create`
+ `pane.spawn`; `signaltty pane read` → `pane.read`; `signaltty wait`
→ `wait`; `signaltty notify` → `notify`. The CLI injects
`$SIGNALTTY_PANE` context for `notify`/`hook-event` when run inside a
pane (env set by the server at spawn).
