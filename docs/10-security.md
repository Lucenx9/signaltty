# 10 — Security Model

The server is security-sensitive: it spawns processes, writes to PTYs,
and accepts automation commands. Threat model: malicious or buggy
**local** content (terminal output, hook payloads, plugin scripts,
notification text) and other local users; no remote attacker in scope
for the MVP (no network listener).

## Boundaries & requirements

- **Local-only**: Unix socket only, under `$XDG_RUNTIME_DIR/signaltty/`
  (dir 0700, socket 0600, user-owned). No TCP listener by default; any
  future remote bridge must be explicit, authenticated, and documented.
- **Peer identity**: `SO_PEERCRED` check — refuse connections whose
  uid ≠ server uid, even if filesystem perms were loosened.
- **Input validation**: every IPC param validated (id format, size
  clamps, argv non-empty + executable lookup, signal allowlist,
  base64 bounds, 16 MiB line cap). Malformed input → error, never panic.
- **Kill scope**: `pane.signal` defaults to the child pid only;
  process-group kill requires explicit `group: true` and only targets
  the pane's own pgid. No arbitrary-pid signalling.
- **Spawn scope**: `pane.spawn` runs as the server user with cwd
  constrained to directories the user can access; env overrides pass
  an allowlist + blocklist (`LD_PRELOAD`, `LD_LIBRARY_PATH` stripped
  unless explicitly permitted in server config).
- **Automation boundaries**: `pane.input`/`pane.read` are the
  orchestration primitives — available to any local client of the
  user's own socket. Document that any process running as the user can
  drive panes; this matches tmux semantics. Future macaroons/scopes
  (read-only automation tokens) are a Phase 4 hardening item, not MVP.
- **Notification sanitization**: title/body stripped of C0/C1 controls
  (except `\n\t`), OSC/CSI sequences neutralized, length-capped
  (title 200, body 2000 chars) before storage, IPC fan-out, or desktop
  delivery. Desktop actions carry only opaque ids — clicking never
  executes embedded commands.
- **Hook payloads**: treated as untrusted JSON; size-capped (256 KiB),
  parsed strictly, unknown fields ignored; hook shims run with user
  privileges and must be user-installed (installer never overwrites
  user hook files — merges/appends with markers).
- **Terminal output**: the server's OSC scanner only *reads*; extracted
  strings go through the same sanitizer. VTE/GUI rendering of untrusted
  bytes is the widget's job (VTE already handles this).
- **Persistence**: `state_dir` 0700; snapshot contains no secrets by
  construction (env allowlist); scrollback tails may contain
  credentials the user pasted — file perms 0600, documented, with a
  `redact` option to disable tail persistence per workspace.
- **Executable plugins (Phase 4)**: run with full user privileges;
  install is explicit and the trust requirement is documented at
  install time. No sandboxing in MVP; investigated later.

## Non-goals (MVP)

Multi-user servers, network exposure, privilege separation between
server/workers, secret redaction inside scrollback content, audit log
(signing). Revisit if a network listener is ever added.
