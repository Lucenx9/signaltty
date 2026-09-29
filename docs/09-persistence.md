# 09 — Persistence & Restore Strategy

Honest rule: **processes cannot survive reboot, power loss, or server
death.** We persist structure + resume metadata, never pretend
otherwise.

## What is persisted

Snapshot file (atomic write: tmp + fsync + rename) at
`$XDG_STATE_HOME/signaltty/snapshot.json`, debounced (≤1 write/2s)
plus on every structural change and on SIGTERM:

- server: `version`, `saved_at`, `protocol`
- workspaces: `id, name, cwd, tabs[], active_tab_id, git cache`
- tabs: `id, workspace_id, title, layout tree, active_pane_id`
- panes: `id, workspace_id, tab_id, title, cwd, argv, env allowlist,
  pty_size, agent{kind, agent_session_id, resume_argv, model},
  lifecycle, attention, last_message, created_at`
- scrollback tail per pane: last ~64 KiB to
  `history/<pane_id>.tail` (best-effort context, not full history)
- notifications: last 200, with `read_at`

Never persisted: PTY master fds, child pids (stale after restart),
secrets (env allowlist only: `TERM`, `LANG`, `SIGNALTTY_*`, agent
non-secret config; never tokens/keys).

## Restore states (shown distinctly in UI/CLI)

| State | Meaning |
|---|---|
| `LIVE` | child running, PTY attached (normal) |
| `RESTORED` | structure rebuilt after restart, no process yet |
| `RESUMABLE` | `RESTORED` + adapter has official resume argv + session id |
| `EXITED` | process ended pre-restart or tombstoned (scrollback kept) |

## Restore policy

1. On server start with a snapshot: rebuild workspaces/tabs/layouts
   and panes as `RESTORED`/`RESUMABLE`/`EXITED` tombstones with saved
   titles, sizes, scrollback tails, lifecycle/attention as of save
   (attention preserved — unread stays unread).
2. **Never auto-run arbitrary saved shell commands.** Auto-resume is
   allowed only for adapter-owned official resume argv *and* only with
   explicit user opt-in (per-workspace `auto_resume: true`, default
   false). Otherwise each resumable pane offers one-key resume.
3. Resume = adapter builds argv from persisted `agent_session_id`
   (e.g. `claude --resume <id>`, `codex resume <id>`,
   `opencode --session <id>`, `cursor-agent --resume <id>`) and the
   server spawns it as a fresh PTY child in the same pane slot,
   preserving id, title, and history tail.
4. If the agent binary/session is gone, resume fails visibly with the
   adapter error; the tombstone remains with its tail.

## Crash safety

- Snapshot writes are atomic; loader tolerates missing/corrupt files
  (backs up corrupt snapshot, starts empty, logs loudly).
- `snapshot_version` field; unknown future versions refuse to load
  with a clear error rather than misinterpreting.
- Optional periodic snapshots every 60s regardless of activity.

Official resume also restores the three allowlisted provider config-directory
overrides in `agent.config_env`; missing fields in older snapshots default empty.
No credentials or arbitrary environment values are persisted (ADR-0013).
