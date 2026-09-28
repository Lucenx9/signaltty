# 13 — Plugins & Workflows (Phase 4)

Executable plugins over the stable `signaltty/1` API. No WASM, no
in-process ABI: a plugin is a directory with a `plugin.toml` manifest
and executable scripts. See ADR-0007.

## Layout

```text
~/.config/signaltty/plugins/<name>/
  plugin.toml     # manifest (TOML)
  hook.sh         # event hook entrypoint (example)
  fanout.sh       # [[command]] entrypoint (example)
```

Plugin dir resolution: `signaltty-server --plugin-dir`, else
`$SIGNALTTY_PLUGIN_DIR`, else
`$XDG_CONFIG_HOME/signaltty/plugins`. The manifest `plugin.name`
must match the directory name. Broken manifests fail only their own
package and show up in `plugin.list` → `failures`.

## Manifest schema

```toml
[plugin]
name = "event-log"          # [a-z0-9_-]{1,64}, must match dir name
version = "0.1.0"
description = "optional"

[[hook]]
events = ["agent.done", "attention.*"]  # exact, prefix.*, or *
command = ["./log.sh", "--syslog"]      # argv; argv[0] with / resolves in dir
timeout_secs = 10                       # default 10, max 300, min 1

[hook.env]
EVENT_LOG = "/tmp/signaltty-events.log"

[[command]]
name = "spawn"              # [a-z0-9_-]{1,64}, unique per plugin
description = "optional"
run = ["./fanout.sh"]       # argv, same resolution as hooks
```

## Event hooks

The server offers every broadcast event (except high-volume
`pty.data` — hooks that need output call `pane.read`) to hooks
whose `events` globs match (`*`, `prefix.*`, exact).

Protocol per firing:

- cwd = plugin directory.
- stdin = one event envelope JSON:
  `{"protocol":"signaltty/1","type":"event","event":...,
  "seq":N,"payload":{...}}` (same shape as socket events).
- env: `SIGNALTTY_SOCKET`, `SIGNALTTY_EVENT` (event name),
  `SIGNALTTY_PLUGIN_DIR`, `SIGNALTTY_PLUGIN_NAME`, plus `hook.env`.
- exit code is advisory: nonzero/timeout is logged server-side and
  counted in `plugin.list` stats (`runs`, `errors`, `last_error`,
  `last_run`); it never affects the event bus.
- at most 8 hooks run concurrently; excess firings queue.

Reload without restart: `signaltty plugin reload` (stats reset).

## Commands

`signaltty plugin run <name> <command> -- [args...]` executes the
manifest `run` argv locally with extra args appended, cwd = plugin
dir, stdio inherited, `SIGNALTTY_SOCKET` exported to the CLI's
socket. This is the pane-launcher / workflow-automation primitive:
scripts orchestrate through the same CLI every user uses.

## Trust model

Hooks and commands run as your user with your environment. Only
install plugins you trust; treat the plugin dir like `~/.local/bin`.
No sandboxing in v1. `plugin.list` shows exactly what is loaded.

## Orchestration patterns

Agents orchestrate through semantic calls, not PTY scraping:

- fan-out: `new` + `pane split` per worker (see
  `examples/plugins/fanout/`).
- barrier: `wait --pane <id> --until done` per worker.
- collect: `pane read` tails, then `notify` once.
- full recipe: `examples/workflows/fanout-review.sh`.

CLI surface: `signaltty plugin list|reload [--json]`,
`signaltty plugin run <name> <command> [-- args...]`;
socket surface: `plugin.list`, `plugin.reload` (see 08).
