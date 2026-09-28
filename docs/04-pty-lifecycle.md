# 04 — PTY Lifecycle

The server — never the GUI — owns PTYs: spawn, master/slave setup,
resize, signals, exit reaping, scrollback, and client attach/detach.

## Spawn

1. Client sends `pane.spawn {workspace_id, tab, cwd, argv, env, cols, rows}`.
2. Server `forkpty`-equivalents via `portable-pty`:
   `openpty → fork → setsid → slave as controlling tty → exec argv`.
3. Server registers pane, starts async reader tasks:
   - output pump: PTY master → scrollback ring + VT parser (screen
     model, OSC/title/BEL extraction) → broadcast to attached clients.
   - exit watcher: EOF + `waitpid` → mark `exited`, freeze scrollback,
     emit `pane.exited`.
4. Server replies with the opaque `pane_id`.

Environment: pane inherits a sanitized copy of the server environment
plus per-request overrides. `TERM` defaults to `xterm-256color` unless
the client overrides.

## I/O paths

- **Output**: PTY master → server ring buffer (bounded, e.g. 5k lines +
  1 MiB cap per pane, configurable) → VT parser → attach clients get
  a snapshot (screen + recent scrollback) then a live byte stream.
  Parsing never blocks the pump: bounded channels, drop policy is
  "clients lag, history doesn't" — slow clients get snapshots, the
  ring always advances.
- **Input**: client `pane.input {pane_id, data}` → server validates
  (pane exists, live) → writes to PTY master. No PTY input echo
  synthesis; the kernel/child echoes.

## Resize arbitration

| Situation | Policy |
|---|---|
| No client attached | Keep last size; never shrink to 0. Default 80×24 at spawn. |
| One client | Client size wins (clamped to 20–500 cols, 5–200 rows). |
| Multiple clients | Last-writer-wins; server broadcasts the new canonical size so all clients reflow consistently. GUI shows a subtle "shared view" hint when >1 viewer. |

Resize = `ioctl(TIOCSWINSZ)` + `SIGWINCH` to the foreground process
group (handled by `portable-pty`), then `pane.resized` event.

## Exit & signals

- EOF on master + child reaped → `live=Exited{code}`, keep scrollback
  and metadata (pane becomes a tombstone until closed or workspace
  pruned). Adapters map exit to `failed`/`done` only for one-shot
  commands; interactive agents exiting is `exited` + attention `unread`.
- `pane.signal {pane_id, SIGINT|SIGTERM|SIGKILL|…}` → server sends to
  the child pid (and optionally process group; default: child only,
  group on explicit flag).
- Server shutdown: refuses while panes live unless `--force`; on
  SIGTERM it snapshots state, leaves children running if configured
  `survive_server_restart=false`? No — children die with the server
  (they are its children). Honest semantics: server death = process
  death; only structure + resume metadata persist (see [09](09-persistence.md)).

## Attach / detach / reconnect

- `attach {pane_id, cols, rows}` → server replies with
  `{snapshot: <screen text + scrollback tail>, size, live, lifecycle,
  attention}` then streams `pty.data` events.
- `detach` (or socket drop) → server just removes the subscriber.
  The PTY, child, and ring are untouched.
- Crash of a client = socket EOF = implicit detach. Server must never
  panic on malformed client bytes; it drops that connection and logs.

## Scrollback & history

- Per-pane bounded ring (lines + byte cap). Persisted tail (last N Kb)
  to `state_dir` for post-restart context; full history is best-effort.
- Screen model (headless VT state) maintained server-side so `pane.read`
  and automation work without any GUI attached.
