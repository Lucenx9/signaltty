# 07 — Agent Integration Strategy

Layered detection, highest-signal first. Explicit signals always
override heuristics. Unknown apps remain fully working terminals.

## Priority stack

1. **Native integration** — agent exposes state over a local API the
   adapter can poll/subscribe (rare today; e.g. OpenCode `serve` HTTP,
   Codex `app-server`). Best-effort where officially documented.
2. **Agent hooks / semantic events** — the primary mechanism for
   Phase 2. Small shims installed into each agent's hook system call
   `signaltty hook-event --agent <kind> --event <name> --pane $SIGNALTTY_PANE`
   (pane id injected via env at spawn) with the hook JSON on stdin.
3. **Standard terminal notifications** — OSC 9 (iTerm2/ConEmu),
   OSC 99 (Kitty), OSC 777 (rxvt/Ghostty/WezTerm) scanned from the PTY
   byte stream server-side. Any CLI/script can emit them; zero config.
4. **Foreground process info** — `/proc`: argv, cwd, child tree, CPU.
   Identifies *which* agent runs (`codex`, `claude`, `opencode`,
   `cursor-agent`) and whether it is forked/busy.
5. **Terminal state** — title (`OSC 0/1/2`), BEL, cursor, alt-screen.
   Cheap, no parsing of natural language.
6. **Output heuristics** — last resort only (spinners, prompt shapes).
   Never the architecture's foundation: declarative `[[screen]]` manifest
   rules, applied only to panes whose current process has sent no hook
   (see [Screen rules](#screen-rules-layer-6)).

## Adapter interface

```rust
trait AgentAdapter: Send + Sync {
    /// Does this adapter own the pane's foreground process?
    fn identify(&self, proc: &ProcessInfo) -> bool;
    /// Map a semantic event / poll snapshot to lifecycle + attention.
    fn lifecycle_state(&self, ev: &AdapterEvent) -> LifecycleDecision;
    /// Native session id, if the agent reports one.
    fn session_identity(&self, ev: &AdapterEvent) -> Option<String>;
    /// Map hook payload / OSC / exit to a notification (or none).
    fn notification_event(&self, ev: &AdapterEvent) -> Option<NotificationDraft>;
    /// Official resume argv for a persisted session id, if supported.
    fn resume_capability(&self, session_id: &str) -> Option<ResumeCommand>;
    /// Static metadata: kind, display name, hook install recipe.
    fn metadata(&self) -> AdapterMetadata;
}
```

Adapters: `CodexAdapter`, `ClaudeCodeAdapter`, `OpenCodeAdapter`,
`CursorAdapter`, `PiAdapter`, `GenericTerminalAdapter` (process + title +
BEL only).

## Per-agent integration (verified Sept 2026, local CLIs)

### Codex CLI (`codex`)

- Resume: `codex resume [SESSION_ID]`, `codex resume --last`;
  non-interactive `codex exec resume <id|--last>`; `codex queue`,
  `codex fork`, `codex archive`. Session ids are UUIDs/names.
- Hooks: `~/.codex/hooks.json` / `[hooks]` in `config.toml`; events
  include `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
  `PermissionRequest`, `PostToolUse`, `Stop`, `SessionEnd`,
  `SubagentStop`. Payload on stdin carries `session_id`,
  `transcript_path`, `cwd`, `hook_event_name`, `turn_id`.
  No `Notification` event — use `notify = ["signaltty","hook-event",…]`
  (fires on `agent-turn-complete`) plus `tui.notifications` → BEL.
- Session env: `CODEX_THREAD_ID` equals Stop payload `session_id`.
- Shim: `notify` entry + hook commands calling `signaltty hook-event
  --agent codex`. Lifecycle: `UserPromptSubmit`→working,
  `PermissionRequest`→blocked+permission_required, `Stop`→done+unread.
- Verified against codex 0.157.1 (Sept 2026): hooks are trust-gated —
  new/changed entries sit at "review required" until the user picks
  "Trust all and continue" in an interactive session (hash recorded
  under `[hooks.state]` in config.toml); `codex exec` cannot grant
  trust, but `codex exec --dangerously-bypass-hook-trust` fires hooks
  (SessionStart/UserPromptSubmit/Stop/SessionEnd all observed). Hook
  timeouts clamp to 3s — our installer uses 3. The `Stop` hook
  REQUIRES stdout to be empty or JSON on exit 0 (plain text fails the
  hook), so `signaltty hook-event` prints human feedback to stderr and
  keeps stdout silent unless `--json`.

- Codex 0.159.1 can reuse a detached shared daemon that retains a previous
  terminal's `SIGNALTTY_PANE` / socket. Hooks run with the daemon environment,
  causing `hook exited with code 1` (`NO_SUCH_PANE`) or updating another pane.
  Direct managed local interactive launches, split and resume use native
  `--no-daemon` when supported (bounded version probe); original stored argv,
  config/auth/trust and hook commands remain unchanged. Unsupported clients or
  explicit remote sessions retain their command with an integration notice.
  Exec/utility commands are unchanged. See [ADR-0014](adr/0014-codex-pane-runtime.md).
  When typing Codex in an ordinary shell, use `codex --no-daemon`. To recover an
  existing session, exit the TUI and run `codex --no-daemon resume --last` in the
  same pane. Already-running clients keep their backend until restarted;
  Signaltty does not stop the shared daemon or bypass hook trust.

### Claude Code (`claude`)

- Resume: `claude --resume [id]`, `-c/--continue`, `--session-id <uuid>`
  for pinned new sessions; `--settings <file>` for hook injection;
  `-p/--print` non-interactive; `claude attach/logs/stop/rm/agents`.
- Hooks: settings JSON (`~/.claude/settings.json`, project/local,
  `--settings`); events `SessionStart`, `UserPromptSubmit`,
  `PreToolUse`, `PostToolUse`, `Stop`, `SubagentStop`, `Notification`
  (`idle_prompt`), `PreCompact`. Payload `session_id` equals
  `$CLAUDE_CODE_SESSION_ID`; survives `-p --resume`.
- Shim: settings snippet installed by `signaltty integration install
  claude` (user-scoped, non-destructive merge). `Notification` →
  attention directly; `Stop` → done+unread.

### OpenCode (`opencode`)

- Resume: `opencode --session <id>` / `-s`, `--continue` / `-c`;
  headless `opencode run --session <id>`; `opencode serve` + HTTP API;
  `opencode attach`, `opencode session`, `opencode export`.
- Hooks: JS/TS plugin system (`~/.config/opencode/plugins`,
  25+ events): `session.created/updated/idle/status`,
  `command.execute.before`, `event`, `config`. Plugins receive
  `properties.sessionID` and can shell out.
- Shim: a small plugin file calling `signaltty hook-event --agent
  opencode`. `session.status busy/idle` → working/done; idle →
  done+unread. `serve` API is the future native-integration path.
  V1 creation identity also comes from `properties.info.id`; nested provider
  error messages are preserved. V2 `session.execution.failed` normalizes to
  `session.error` (failed + error attention), while success and interruption
  retain idle completion behavior. Installed-plugin tests execute both loaders
  rather than only checking the generated source shape. Event contracts:
  [OpenCode 2.0.25](https://github.com/anomalyco/opencode/blob/v2.0.25/packages/schema/src/session-event.ts).

### Cursor Agent (`cursor-agent`)

- Resume: `--resume [chatId]` (picker when bare), `--continue`;
  `--print --output-format json|stream-json` headless; `--model`,
  `--mode plan|ask`.
- Hooks: `~/.cursor/hooks.json`; `sessionStart` includes a stable
  `session_id` mappable to the CLI pid (must reject Desktop-origin
  events via process ancestry check).
- Shim: `hooks.json` entry + reporter script. Screen/title fallback
  aligned with Codex (session-only pattern, no output regex).

### Pi (`pi`)

- Resume: `pi --session <path|id>` (partial UUID ok), `-c/--continue`,
  `-r/--resume` picker; `--no-session` for ephemeral runs.
- Detection: a Node script that sets `process.title`, so
  `/proc/<pid>/cmdline` reads `pi` and shell-launched sessions are
  promoted too.
- Hooks: none yet. Bundled screen rules report `working` and `done`
  (see [Screen rules](#screen-rules-layer-6)); title/BEL/exit work as for a
  plain terminal. With no reported session id, resume is never offered.
  Integration path: a pi extension calling `signaltty hook-event`.

## Session identity & resume (see also [09](09-persistence.md))

- Identity arrives via hook payload/env only — never scraped from
  terminal text while a better channel exists. Exception: resumed
  sessions may fire no `SessionStart` (observed: `muse resume` emits
  only prompt/Stop); the server therefore also accepts
  `signaltty report-session --pane <id> --session <sid>` and keeps the
  last known id across respawns in the same pane.
- `resume_capability()` returns the official argv (table above). The
  server persists it but never auto-runs it: restore offers
  one-click/keypress resume per pane (LIVE/RESTORED/RESUMABLE/EXITED).

## Claude permission-channel probe

A live probe on 2026-10-02 verified Claude `PermissionRequest` Once/Deny
verdicts, including two simultaneous signaltty panes. It also records terminal
answers, timeout fallback, and ignored late verdicts. The experiment ran against
`eaf8076` with a response file standing in for the GUI choice. It does not
independently verify the subsequently implemented native response bridge.
See [probe results](../specs/003-inline-approvals/probe-results.md) and the
[repeatable experiment](../specs/003-inline-approvals/quickstart.md).

## Hook attribution (pane routing)

`hook-event` resolves its pane in two steps: explicit `pane_id`
(shims read `$SIGNALTTY_PANE`, injected by the server at spawn), else
process-ancestry fallback — the CLI sends its pid as `client_pid` and
the server walks `/proc` parents to the deepest live pane child. The
fallback survives sandboxed agents that strip hook env, and safely
ignores foreign hooks (Cursor Desktop shares `hooks.json` but its
processes are never our descendants → accepted, classified nothing).

## Session end after review

Claude/Codex `SessionEnd` and Cursor `sessionEnd` map to `done` + `unread`
("session ended") for a session that ends mid-turn or before any turn. When the
pane is already `done` or `failed` the hook only restates the outcome: lifecycle,
attention and the useful last message are left as they are, so a read pane stays
`done`/`none` and `failed` is never overwritten with `done`. Session identity and
resume argv from the payload are still recorded. The pane's later PTY exit
likewise does not re-raise `unread` on a reviewed `done`/`failed` pane. See
[03](03-lifecycle-attention.md#clearing-rules).

## Detection overlays: manifests (data, not code)

`$XDG_CONFIG_HOME/signaltty/agents/*.toml` (`SIGNALTTY_AGENTS_DIR`
override, `--agents-dir` on the server) extend one existing adapter kind
without recompiling — wrapper binaries, post-churn hook maps, custom
session keys. Loaded once at startup; malformed files are logged and
skipped, never fatal. `signaltty integration status` lists them.

```toml
[agent]
kind = "codex"              # required: an existing AgentKind
# display_name = "…"        # optional override
# binaries = ["codex-wrap"] # merged with the builtin list

[session]
# key = "thread_id"         # payload session-key override
# resume = ["codex", "resume", "{session_id}"]

[lifecycle.SomeFutureHook]    # any hook; unmapped hooks fall through
# lifecycle = "working"
# attention = "unread"
# message = "…"
```

Each field falls through to the builtin adapter independently when
absent. `kind` must stay inside the typed taxonomy — a genuinely new
agent still needs a Rust adapter. See ADR-0009.

### Screen rules (layer 6)

`[[screen]]` tables classify panes whose current process has sent no hook
(hooks always win, ADR-0006). Every 500 ms the server matches the rules of the
pane's kind, from every manifest of that kind, against the pane title or a
region of the visible screen (lines trimmed, empty lines dropped):

```toml
[[screen]]
id = "approval"             # shown in load errors
state = "blocked"           # working | blocked | idle | hold
region = "bottom"           # bottom (default) | title | screen | top | prompt_box
                            # | above_prompt_box | after_last_rule
                            # | after_last_prompt | before_current_prompt
                            # | without_current_prompt
lines = 12                  # last (top: first) non-empty lines, 1..=200;
                            # bottom and top default to 12
regex = ['Allow\? \[y/n\]'] # any match; (?m) for per-line anchors
# all = ['…']               # every regex must also match
# not = ['…']               # no regex may match
priority = 10               # highest match wins; ties keep file/declaration order
```

A rule needs `regex` or `all`. Regions follow herdr: a horizontal rule is a
line starting with `─` (three or more, or nothing after them); `prompt_box`
is the text between the second-last rule and the next one (empty without
two rules), `above_prompt_box` everything before it (the whole screen
without a box), `after_last_rule` everything below the last rule (the whole
screen without one), `screen` the whole visible screen, `bottom` = `screen`
+ `lines = 12`, `top` the first `lines` (default 12). For Codex's `›` prompt
(a line that is `›` or starts with `› `): `after_last_prompt` is everything
after the last prompt line; the current prompt is the last prompt line with
no `•`, `■`, `✗` or `✓` block line after it, `before_current_prompt` is
everything above it and `without_current_prompt` is the whole screen only
when there is no current prompt (otherwise empty); without a prompt both
prompt regions read the whole screen. A winning `hold` rule leaves the pane's
state unchanged (a transcript viewer or model picker is not a turn state).

signaltty bundles rules for `pi`, `opencode`, `cursor`, `claude` and
`codex`, ported from herdr (`crates/signaltty-agent/screen/`); each ends with
an `idle_fallback` rule (priority -1000, empty regex) so a known agent with no
working or blocked sign reads as idle. Any user `[[screen]]` rule for a kind
replaces that kind's bundled rules; copy the bundled file to adjust it.

`working` → `working`; `blocked` → `blocked` + `input_required`; `idle` after
`working` → `done` + `unread`, after `blocked` → `idle`, otherwise nothing.
Leaving `blocked`, or the first hook taking the pane over, withdraws the
`input_required` a screen rule raised. No match changes nothing. A bad state, region, line count or regex rejects the
manifest at load. See ADR-0024.

## Live process refresh (layer 4)

Spawn argv is only the first word. Every 10s the server re-reads each
live pane's deepest shell-transparent descendant (`/proc`: basename +
cwd): generic panes promote to the detected kind (never demote), and
`pane.cwd` follows `/proc/<pid>/cwd`. One `pane.updated` per changed
pane; silence otherwise (no event spam, no snapshot churn).

## Fallback behavior

No hooks installed, unknown binary, or adapter disabled → pane works
as a plain terminal with `GenericTerminalAdapter`: title tracking,
BEL → `unread`, exit code → `done`/`failed`, OSC notifications still
honored. Zero-configuration agents (raw `claude` with no hooks) still
get OSC + title + exit semantics.

## Automatic configuration (ADR-0013)

Starting a supported agent directly from the New Workspace dialog or `pane.spawn`,
`pane.split`, `pane.resume` prepares its hook integration before the process starts.
Preparation uses the actual executable basename, not `agent_hint` or detection
manifests. The ambiguous `agent` command is generic by default: it may be Grok;
known Cursor aliases can still be declared explicitly in detection manifests.

Only the selected provider is configured. Existing shells and agents already
running are not reconfigured; for agents typed inside a shell use
`signaltty integration install <provider>` first, or start a new workspace with
that agent selected. Restarting an agent may be needed after configuration.

Shared installation merges owned command hooks, preserves foreign hooks/keys,
refuses malformed files and unrelated OpenCode plugins, refreshes stale paths,
and uses bounded advisory locking and atomic replacement. Uninstall removes
only managed commands, including inside mixed groups. Claude `CLAUDE_CONFIG_DIR`,
Codex `CODEX_HOME`, OpenCode `OPENCODE_CONFIG_DIR` and the default XDG OpenCode
location are honored. Explicit CLI `--home` isolates provider home overrides;
OpenCode still honors `XDG_CONFIG_HOME`.

A setup failure keeps the terminal usable and returns `integration.status=error`
with a notice shown in the GUI/CLI. Explicit Claude `--bare` or setting sources
excluding `user`, and OpenCode `--pure`, return `disabled` without installing.
Installation never enables a user's disabled hooks. `configured`/CLI `installed`
mean files contain managed hooks, not that events have fired. For newly written
Codex hooks the notice asks to review/trust them in `/hooks`; Signaltty does not
alter `[hooks.state]`, grant trust or inject trust-bypass flags.

## Native permission replies (ADR-0015)

Managed Claude and Codex `PermissionRequest` hooks use a dedicated waiting
reporter. It converts the provider's `session_id`, `tool_name` and `tool_input`
into a structured prompt with **Allow once** and **Deny**. The GUI or CLI answer
returns through the live reporter as `hookSpecificOutput.decision.behavior`,
not as terminal keystrokes. Reporting-only requests have no native answer route
and render read-only.

The permission reporter has a 125-second provider timeout and a bounded
120-second server wait. No answer, cancellation, supersession, disconnect or
server restart produces no permission verdict, so the provider retains its own
permission flow. Answers validate the request's current pane, session and
deadline. The transport is transient and cannot be restored from an audit log.
Ordinary status reporters retain their short timeouts and silent stdout.

Current Codex 0.159.1 accepts the same narrow allow/deny output as Claude; its
tagged hook engine honors configured timeouts without the earlier three-second
limit. It rejects session permission updates, modified tool input and interrupt
requests, so this integration offers no permanent **Always** policy. Hook trust
remains Codex's own interactive review. The installer preserves foreign hooks
and disabled settings as before.

Sources: [Claude PermissionRequest reference](https://code.claude.com/docs/en/hooks#permissionrequest-decision-control),
[Codex 0.159.1 permission execution](https://github.com/openai/codex/blob/rust-v0.159.1/codex-rs/hooks/src/events/permission_request.rs),
[Codex output parser](https://github.com/openai/codex/blob/rust-v0.159.1/codex-rs/hooks/src/engine/output_parser.rs).

After successful delivery of an answer to a pending decision, the shared Store transition
returns a blocked pane to working for both native permission replies and the
terminal answer channel. Stale answers and non-answer clears do not resume it;
ordinary tool traffic still cannot clear an unanswered blocked state.

When an agent was launched directly through a path-qualified executable,
matching bare resume commands retain that selected path. Explicit manifest
resume paths remain authoritative; promoted shell panes keep the adapter
command. Resume arguments come from the adapter rather than the initial launch
options. Existing bare snapshot commands are repaired when resumed.

Relative executable paths supplied to `pane.spawn` or `pane.split` are anchored
to their launch directory before spawning and stored as absolute paths. Later
process directory changes therefore cannot redirect a session resume.

Legacy snapshots with a relative original executable keep the adapter command:
they do not persist a reliable initial directory to anchor that relative path.
