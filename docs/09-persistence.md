# 09 — Persistence & Restore Strategy

Honest rule: **processes cannot survive reboot, power loss, or server
death.** We persist structure + resume metadata, never pretend
otherwise.

## What is persisted

Snapshot file (temporary write + rename; no fsync) at
`$XDG_STATE_HOME/signaltty/snapshot.json`, debounced (≤1 write/2s)
with structural changes marking a pending save, plus on shutdown/SIGTERM
and, on Linux, on logind's early shutdown notice (ADR-0028):

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
   Self-reported resume argv passed to `report-session` is only executed
   on explicit user `pane.resume`, never automatically at server restart.
   Later hooks or reports for the same agent and session keep it; a new
   agent or session id replaces it with the adapter's command.
3. Resume = adapter builds argv from persisted `agent_session_id`
   (e.g. `claude --resume <id>`, `codex resume <id>`,
   `opencode --session <id>`, `cursor-agent --resume <id>`) and the
   server spawns it as a fresh PTY child in the same pane slot,
   preserving id, title, and history tail.
4. If the agent binary/session is gone, resume fails visibly with the
   adapter error; the tombstone remains with its tail.

## Host shutdown

On Linux the server watches `org.freedesktop.login1` without delaying startup.
`PrepareForShutdown(true)`, or `PreparingForShutdown` already true, writes this
same snapshot while a delay inhibitor is held, then releases the inhibitor and
shuts down. A false notification does not stop the server. If the write fails,
the inhibitor is still released and the server still stops. A missing bus, a
denied inhibit or a logind restart retries with bounded backoff. Dropping the
monitor on any server exit closes the inhibitor. The delay is logind's, not
ours: power loss and SIGKILL remain outside this guarantee. See
[ADR-0028](adr/0028-logind-shutdown-save.md).

## Crash safety

- Snapshot writes are atomic; loader tolerates missing/corrupt files
  (backs up corrupt snapshot, starts empty, logs loudly).
- `snapshot_version` field; unknown future versions refuse to load
  with a clear error rather than misinterpreting.
- Optional periodic snapshots every 60s regardless of activity.
- Rotating snapshot history in `snapshots/`: retains up to 48 historical
  snapshots (at most one preserved every 15 minutes, never preserving an
  empty session) for manual recovery.

Official resume also restores the three allowlisted provider config-directory
overrides in `agent.config_env`; missing fields in older snapshots default empty.
No credentials or arbitrary environment values are persisted (ADR-0013).

Native permission waits are not persisted. A server restart cannot reconnect
an old reporter or deliver an old approval. Restored decisions therefore have
no live native answer channel; the provider owns any new permission prompt.
Git worktree association is recovered from the persisted workspace cwd and
Git's registrations. Closing or restoring a workspace never removes a checkout.
Terminal search and pane zoom are client view state, not saved layout changes.

## Event continuity

The event journal has separate durable sequence reservations and retained-history
evidence; state records are synced before publication. Runtime wait baselines identify
a live process instance and are invalidated by restart, never persisted in Pane. These
guarantees do not strengthen snapshot/tail durability. See
[IPC recovery](08-ipc.md) and [ADR-0018](adr/0018-event-recovery-and-work-baselines.md).
