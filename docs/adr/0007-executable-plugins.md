# ADR-0007 — Executable plugins over the stable socket API

- Status: accepted
- Date: 2026-09-28

Context: Phase 4 needs plugin manifests, event hooks, reusable
workflows, and agent-to-agent orchestration without destabilizing
the validated CLI/socket API or adding a second runtime (WASM).

Decision: plugins are directories with a `plugin.toml` manifest and
executable entrypoints. The server dispatches broadcast events to
`[[hook]]` commands (event JSON on stdin, bounded concurrency,
timeouts, per-hook stats via `plugin.list`/`plugin.reload`); the CLI
runs `[[command]]` entrypoints locally with `SIGNALTTY_SOCKET`
exported. Orchestration is scripts + semantic calls
(new/split/wait/read/notify), not PTY scraping.

Consequences: zero new IPC surface for orchestration; any language
can implement a plugin; hooks run as the user and must be trusted
(no sandbox v1); `pty.data` is excluded from hooks (volume) —
hooks use `pane.read`.
