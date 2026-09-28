# ADR-0001 — Unix socket + JSONL IPC (`signaltty/1`)

- Status: accepted
- Date: 2026-09-28

Context: GUI, CLI, hooks, and future plugins all need one structured,
event-capable local API. Alternatives: D-Bus (heavier typing,
activation semantics we don't need), gRPC/HTTP (overkill locally,
binary deps), custom binary (debugging cost).

Decision: Unix socket at `$XDG_RUNTIME_DIR/signaltty/signaltty.sock`,
newline-delimited JSON, versioned envelope, glob subscriptions, `seq`
replay, base64'd binary. `SO_PEERCRED` uid check.

Consequences: trivially debuggable (`socat` + `jq`), hook shims need
only the CLI, GUI polls nothing. 16 MiB line cap + chunking for PTY
bursts; revisit framing only if profiling demands it.
