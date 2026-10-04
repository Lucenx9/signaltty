# 018 orchestrator — how other tools actually do it

Research date: 2026-10-04. Public repos were shallow-cloned under `/tmp/orch-research` and read there. Nothing else in the signaltty tree was changed.

Labels used below:

- **VERIFIED-IN-SOURCE** — read in the clone named, at the path given.
- **FROM-DOCS** — vendor documentation, not their source. Conductor is closed source, so it is docs only.
- **INFERRED** — a conclusion from the code or docs, not a sentence the project wrote.

## Clones

| Repo | SHA | Commit date | What it is |
| --- | --- | --- | --- |
| pingdotgg/t3code | `f90b77d80971` | 2026-10-03 | Electron/web harness. Unit is a thread, not a PTY. |
| herdrdev/herdr | `5da0a01e1eed` | 2026-10-03 | Rust server that owns a Ghostty VT. Closest behavioral twin. |
| manaflow-ai/cmux | `5f6b50cfc945` | 2026-10-03 | Swift/Ghostty macOS terminal. |
| raine/workmux | `cf8516270afd` | 2026-10-02 | Rust CLI over tmux, wezterm, zellij, kitty. Does not own the terminal. |
| BloopAI/vibe-kanban | `d5cbb5380fa0` | 2026-09-19 | Rust server plus MCP. Task rows and worktrees. Agents are spawned processes, not panes. |
| smtg-ai/claude-squad | `ce1ffb4392b0` | 2026-08-20 (tag 1.0.20) | Go TUI. One tmux session and one worktree per instance. |

Docs fetched the same day:

- Claude Code hooks: https://code.claude.com/docs/en/hooks
- Claude Code headless: https://code.claude.com/docs/en/headless
- Codex hooks: https://learn.chatgpt.com/codex/hooks.md
- Codex `exec`: https://learn.chatgpt.com/docs/non-interactive-mode and https://developers.openai.com/codex/non-interactive-mode.md
- Codex `notify`: https://learn.chatgpt.com/docs/config-file/config-advanced
- Conductor: https://www.conductor.build/docs/ (pages cited inline). This is the Charlie Holtz coding-agent app, not Netflix conductor-oss.

Signaltty already has non-incremental `pane.read`, `wait` with an identity baseline, `worktree.create/remove` that refuses a dirty tree and keeps the branch, and `hook-event`. Those facts are from `docs/07-agents.md` and `docs/08-ipc.md` in this repo (**FROM-DOCS**, this project). The notes below say what to extend, not what to rebuild.

## Direct answers

### 1. How do they know an agent is ready for input, or that a turn finished?

| Tool | Signal | What it is not |
| --- | --- | --- |
| herdr | Hooks via `pane.report_agent`. Screen, OSC title, and OSC progress only when hook authority is off. Idle from the screen is debounced. | Not an idle timeout. |
| cmux | Hook journal folded by a reducer: turn started / completed, approval, question, plan review, error, idle observed. | Hibernation is a killer, not a readiness bit. `waiting_on_human` can stick. |
| workmux | Hooks write `working` / `waiting` / `done` into a JSON file per pane. | Not the screen. Focus clears `waiting` and `done`; that is display, not "ready". |
| t3code | Provider protocol. ACP `stopReason` `end_turn` or `cancelled`. Run status on the thread. | Not a screen and not a PTY. |
| claude-squad | Screen scrape. Hash of `tmux capture-pane` plus a few prompt strings. Auto-yes presses Enter on those strings. | No hooks. |
| vibe-kanban | Process exit and log parsing for the attempt. Kanban column is a separate field. | Not a turn-ready bit. |
| Claude Code | `Stop` = main agent finished a response (not a user interrupt; API errors are `StopFailure`). `PermissionRequest` is immediate. `Notification` `idle_prompt` is about 60s after the reply and only if you look idle. `permission_prompt` waits about 6s. | `idle_prompt` is the wrong hook for "turn finished". |
| Codex | `Stop` with `turn_id` and `last_assistant_message`. `PermissionRequest` for approval. `notify` fires `agent-turn-complete` with the last assistant text. `codex exec --json` emits `turn.started` / `turn.completed` / `turn.failed`. | Transcript file is explicitly unstable. |

### 2. How do they read agent output?

Rendered terminal text: herdr (`ReadSource` visible/recent), cmux (`ghostty_surface_read_text` over viewport/screen/history), workmux and claude-squad (`tmux capture-pane`). None of them has a cursor for "only what is new". herdr's `revision` field is hardcoded `0`. t3code reads stored messages and a snapshot sequence, not the screen. vibe-kanban reads the child process's stdout. Claude and Codex also expose a transcript path; both warn it lags or is unstable, and both put `last_assistant_message` on `Stop` so you do not have to parse the file.

### 3. How does task, worktree, and branch get created and cleaned up?

Nobody uses the word "task" the same way.

- t3code: a thread row holds `branch`, `worktreePath`, lineage, and run status. Delete skips a dirty tree, a moved HEAD, a branch mismatch, a live session whose cwd is inside the checkout, and unfinished outbox work. The branch is kept so resume can recreate the checkout. Submodule failure does not roll the worktree back.
- workmux: the worktree is the handle and exists before the agent, so `wait` can start early. Merge refuses a dirty source you are about to delete, refuses a dirty target, aborts the target on conflict so the target stays clean, and deletes the branch only after a successful merge. A cleanup failure is reported separately from the merge.
- vibe-kanban: a SQLite task outlives the process. Worktree create takes a per-path lock and retries after ripping out stale git metadata. Cleanup force-removes the worktree directory. Docs say merge moves the card to Done and leaves the branch until a manual delete.
- claude-squad: session title becomes a branch from current HEAD (not from a fetched base). Kill force-removes the worktree and `git branch -D` unless the branch already existed. Pause removes the worktree and keeps the branch. Diff is against the commit recorded at setup, after `git add -N`.
- herdr: `git worktree add` / `remove`. Dirty is detected by matching git's English stderr. The remove command does not delete the branch.
- Conductor (**FROM-DOCS**): one workspace per shippable unit. New branch from the configured base after `fetch origin`. Chats, diffs, and PR stay on the workspace. Archive leaves the active list and can be restored with chat history. The docs do not say whether archive deletes the worktree or the branch.

### 4. How do children report results?

- t3code: a context transfer of type `subagent_result`, plus a delivery state `pending → claimed → acknowledged → delivered → disposed`. A restart can hold the result so a cut continuation is not reported done.
- cmux: `agent.message` with states `queued → delivered → read` (or `failed`). Delivery is a hook that injects text. It never types into the pane. Body is fenced with the message id. Headless `claude -p` / `codex exec` must not claim the interactive inbox. Codex cannot be woken once idle, so a queued message is delivered by a `Stop` hook that returns `decision: block`.
- herdr: no result payload. A search of `src` for `parent_pane`, `spawned_by`, and `report_result` found nothing. The child "reports" by going idle/done; the parent reads the screen or waits on status.
- workmux: no result command. A search of `src` for a report/result API found only cleanup diagnostics. The parent waits on hook status and re-reads the pane.
- Claude `Stop` / Codex `Stop` carry `last_assistant_message`. Claude `SubagentStop` also carries `agent_transcript_path`. `claude -p --output-format json` returns `result` and `session_id`. `codex exec` prints the final message on stdout, or a JSONL stream ending in `turn.completed` when `--json` is set.
- claude-squad and Conductor: no child-to-parent channel in the code or the docs that were read.

---

## A. Reliable pane reading

### herdr — rendered VT, no cursor

**VERIFIED-IN-SOURCE** `src/api/schema/common.rs`, `src/app/api/panes.rs`, `src/app/api/agents.rs`, `src/pane.rs`, `docs/next/website/src/content/docs/agent-automation.mdx`.

`pane.read` / `agent.read` return rendered text. Sources: `Visible`, `Recent`, `RecentUnwrapped`, `Detection`. Formats: `Text` or `Ansi`. Default recent window is 80 lines, cap 1000. Result fields include `pane_id`, `workspace_id`, `tab_id`, `source`, `format`, `text`, `revision`, `truncated`.

`handle_pane_read` and `handle_agent_read` set `revision: 0`. There is a `content_seq` used to pair selection updates and to skip an idle rescan when the screen has not changed (`src/pane/agent_detection.rs`). It is not returned as a read cursor.

`pane.wait_for_output` polls `pane.read` and matches a substring or regex (`src/api/wait.rs`). Client disconnect ends the wait.

Alt-screen: a recent read may page via mouse-scroll only when the agent is idle. If history is required and the agent is not idle, the API returns `agent_not_idle`. A passive read does not move the viewport.

Copy: rendered text, the alt-screen refusal, passive vs interactive intent. Avoid: treating `revision` as a cursor. It is always zero.

### cmux — Ghostty regions, last N lines

**VERIFIED-IN-SOURCE** `Packages/macOS/CmuxTerminal/Sources/CmuxTerminal/Surface/TerminalSurface+TextCapture.swift`, `TerminalTextRegion.swift`, `CLI/cmux.swift` around the `read-screen` / `capture-pane` path.

`readText` calls `ghostty_surface_read_text` over `viewport | screen | history | active`. CLI `read-screen` / `capture-pane` map to `surface.read_text`. `--lines N` implies scrollback and returns the last N lines. There is no byte offset.

`surface.input_state` is the companion read for "is there a draft": `empty | draft | dialog | unknown`, plus `draft_length`, `agent`, `lifecycle` (`unknown | running | idle | needsInput`), `waiting_on_human`, `blocks_typing`. Documented in `docs/cli-contract.md`.

Copy: region reads and a draft/dialog bit next to the text. Avoid: a line window as the only incremental channel.

### workmux and claude-squad — `tmux capture-pane`

**VERIFIED-IN-SOURCE** workmux `src/multiplexer/tmux.rs` (`tmux capture-pane -p -e -S -N -t pane`), `src/command/capture.rs` (strip ANSI, drop trailing blanks, keep last N). claude-squad `session/tmux/tmux.go` `CapturePaneContent` uses `tmux capture-pane -p -e -J -t name`. Optional `-S`/`-E` exist. No cursor in either.

claude-squad `HasUpdated` hashes the captured pane and also sets `hasPrompt` if the text contains one of: Claude `"No, and tell Claude what to do differently"`, aider `"(Y)es/(N)o/(D)on't ask again"`, Gemini `"Yes, allow once"`. That hash is a change detector for the preview ticker, not a read API.

Copy: capture rendered lines, strip ANSI at the edge. Avoid: prompt-string matching as a read protocol.

### t3code — event sequence, not a screen

**VERIFIED-IN-SOURCE** `apps/server/src/mcp/OrchestratorMcpService.ts` (`incrementalThreadRead: true`), `packages/client-runtime/src/state/shell.ts` (`afterSequence` / `snapshotSequence`). Output is provider events and stored messages (`agent_message_chunk`, ACP `stopReason`). No PTY read was found.

`CheckpointDiffQuery.getTurnDiff` (`apps/server/src/checkpointing/CheckpointDiffQuery.ts`) diffs two ready checkpoints whose runs are `completed`. Turn 0 is the `root_run` scope, and the comment says that ordinal stays the baseline while `runId` tracks the latest owner. The diff is checkpoint-to-checkpoint, not `git diff HEAD`.

Copy: a monotonic sequence over structured events, and a diff against a recorded baseline. Do not pretend a screen cursor is that sequence.

### vibe-kanban — process logs

**VERIFIED-IN-SOURCE** `crates/executors/src/logs/plain_text_processor.rs` line-buffers stdout. Executors (`crates/executors/src/executors/claude.rs`, `codex.rs`) spawn a process. This is the right model only when `task start` launches a non-interactive agent (`claude -p`, `codex exec`), not a pane.

### Claude and Codex transcripts

**FROM-DOCS** Claude hooks: `transcript_path` is under `~/.claude/projects/.../<session>.jsonl`. The file is written asynchronously and may not yet contain the current turn when a hook fires. `Stop` and `SubagentStop` include `last_assistant_message` for that reason. Subagent transcripts live in a nested `subagents/` folder (`agent_transcript_path`).

**FROM-DOCS** Codex hooks: `transcript_path` "isn't a stable interface for hooks and may change over time." `Stop` includes `last_assistant_message`. Example path in the SessionEnd sample is `/workspace/.codex/rollout.jsonl`.

Do not make either file the primary read.

---

## B. Reliable input, wait, and status

### herdr — the input path to copy

**VERIFIED-IN-SOURCE** `src/app/api/agents.rs`, `src/app/api_helpers.rs`, `src/api/wait.rs`, `src/pane.rs`, `src/cli/spec.rs`.

`agent.prompt` takes `target`, `text`, and optional `wait: { until, timeout_ms }`.

Refusals before any write:

- empty text
- agent `blocked` → `agent_blocked`
- unknown, `launch_pending`, or foreground mismatch → `agent_not_ready`

Text and Enter are separate. Bracketed paste wraps the text (`\x1b[200~` … `\x1b[201~`) when paste bracketing is on. Enter is its own encoded key, delayed by `AGENT_PROMPT_SUBMIT_DELAY` = 300ms.

Two agent-specific exceptions, both in the submit path:

- Windows Codex: after the paste, send a non-character key (Right). Codex's paste burst rewrites a following Enter into a newline until the idle flush.
- Copilot: send focus-gained before submit, or it ignores a synthetic Enter.

When `wait` is set, `prompt_agent` snapshots the agent before submit. It requires a transition into `Working` or `Blocked` within `AGENT_PROMPT_EFFECT_TIMEOUT_MS` = 5000ms, else `agent_prompt_stalled`. Then it waits for the requested settled status. The CLI spec says this does not track turns: if the agent is already working, that active turn's completion can match. Identity pin is terminal id + name + agent. Pane moved, closed, exited, or agent replaced → not running. Events are replayed from the pre-submit sequence so a lifecycle event consumed by the activity gate can still finish the settled wait. `accept_transient_status` applies only to the activity gate.

Statuses: `Idle`, `Working`, `Blocked`, `Done`, `Unknown`. `Done` means idle and unseen (`pane_agent_status` in `src/app/api_helpers.rs`). Attention rank: Blocked 4, idle unseen 3, Working 2, idle seen 1.

Hook authority: `full_lifecycle_hook_authority_active` skips screen detection. Otherwise manifests match screen + OSC title + OSC progress. Screen working→idle is debounced: 100ms recheck, 3 confirmations, cap 700ms (`src/pane/agent_detection.rs`), plus a 3s startup grace. Hook reports carry `seq`; a stale seq is dropped. `interactive_ready` and `launch_pending` are fields on the agent info. `state_change_seq` and `completion_seq` (the idle transition that completed work) are independent of whether a viewer has seen it.

`pane.report_agent` fields: `pane_id`, `source`, `agent`, `state`, optional `message`, `seq`, session id/path, `resume_argv`.

Copy all of this, including the 5s "did the prompt land" gate and the refusal to type into a blocked agent. Signaltty's existing `wait.after` baseline is the same idea; extend it to cover the submit race rather than adding a second wait.

### cmux — draft guard, no agent wait

**VERIFIED-IN-SOURCE** `docs/cli-contract.md`, `CLI/cmux.swift`.

`send` refuses to type over `draft` or `dialog` unless `--force`. An Enter-only send is allowed over a draft. `waiting_on_human` does not itself block typing, and the docs say it can stick after an interrupt or an API error.

Paste: Ghostty brackets if bracketed paste is enabled; otherwise newlines become Enter and unsafe controls become spaces. `paste --submit` presses an agent-aware submit key after the paste. The draft guard still applies.

Lifecycle reducer (`Packages/macOS/CmuxAgentJournal/.../AgentLifecycleReducer.swift`, `AgentLifecyclePhase.swift`, `AgentLifecycleReducerState.swift`):

- Phases: `unknown`, `running`, `backgroundWorkPending`, `needsInput`, `idle`, `error`.
- `sessionStarted` → unknown, `turnStarted` → running, `turnCompleted` / `idleObserved` → idle, or `backgroundWorkPending` if work is pending.
- Approval, question, plan review → `needsInput`. `errorReported` → error. `sessionEnded` keeps the phase and sets `ended`.
- Subagent events do not drive the hosting pane (`isSubagent` is ignored for the badge).
- Stale sequence and older `occurredAt` are dropped, except an explicit `stateChanged`.
- Several live sessions on one surface combine by precedence: running > backgroundWorkPending > needsInput > error > unknown > idle (`combinedPhase`).

Hibernation (`docs/agent-hooks.md`): idle, plus no output/input/lifecycle for `idleSeconds` (default 5), plus about 60s of an unchanged tail and process fingerprint, plus no background shells or unfinished transcript tasks. That kills the surface. It is not a "ready to type" signal.

CLI `wait` in this clone is `cmux vm wait` and `browser wait` (`CLI/cmux.swift` cases at the vm switch and the browser switch). There is no agent-readiness wait verb in those dispatch tables.

Copy: the draft/dialog guard, and the precedence that a running session outranks `needsInput`. Avoid: treating `waiting_on_human` or hibernation as ready, and treating subagent events as the parent pane.

### workmux — hooks for status, send does not check ready

**VERIFIED-IN-SOURCE** `resources/codex/hooks/workmux-status.json`, `.claude-plugin/plugin.json`, `src/state/types.rs`, `src/command/set_window_status.rs`, `src/command/wait.rs`, `src/command/send.rs`, `src/multiplexer/mod.rs`.

Codex hook map:

- `UserPromptSubmit`, `PostToolUse`, `SubagentStart` → `working`
- `PermissionRequest` → `waiting`
- `SubagentStop` → `working` (a child finishing does not mark the parent done)
- `Stop` → `done`

Claude plugin map:

- `UserPromptSubmit`, `PostToolUse` → `working`
- `Notification` matcher `permission_prompt|elicitation_dialog` → `waiting`
- `Stop` → `done`
- `SessionStart` matcher `startup|resume|clear|fork` → `workmux register-agent`

`AgentState` file, one per pane, name `{backend}__{percent-encoded instance}__{pane_id}.json`:

- `pane_key` {backend, instance, pane_id}
- `workdir`, `status`, `status_ts`, `activity_ts`
- `pane_title`, `pane_pid` (shell pid, to detect pane-id recycling)
- `command` (foreground command at status-set time; if it changes, the agent exited)
- `updated_ts`, `window_name`, `session_name`
- `boot_id` (mux server boot; distinguishes close from crash)
- `agent_kind`
- `agent_session_id` (binds a pane-less hook event back through process ancestry)

`waiting` and `done` auto-clear when the window is focused. Do not copy that as a readiness bit.

`wait` polls every 2s. It resolves the worktree path from local git first, so you can wait before the agent exists. Target status is `working | waiting | done`. Any agent in that worktree with the status counts. If an agent was seen and then disappeared: worktree path gone → treat as merged success; path still exists → exit 3 "agent exited unexpectedly". Timeout exits 1. There is no identity baseline and no "must leave idle first" gate.

`send` splits paste and Enter (bracketed paste with no delay, or multiline paste + 100ms + Enter, or keys). A leading `!` gets a 50ms delay on some profiles. It does not refuse a working or blocked agent, and it has no draft guard.

Copy: the persisted state fields (`pid`, `command`, `boot_id`, `agent_session_id`) and the "path gone vs path remains" crash distinction. Copy the hook map, especially `SubagentStop → working`. Avoid: the 2s poll as a lost-prompt detector, focus-clear, and send-without-refusal.

### t3code — provider turn, not keystrokes

**VERIFIED-IN-SOURCE** `packages/contracts/src/orchestrationV2.ts`, `apps/server/src/provider/acp/AcpSessionRuntime.ts` and its tests, `apps/server/src/mcp/OrchestratorMcpService.ts`.

Run statuses: `preparing`, `queued`, `starting`, `running`, `waiting`, `completed`, `interrupted`, `failed`, `cancelled`, `rolled_back`. Active is the first five. Archive is refused while `preparing | starting | running`. `queued` is archivable only when `activeRunId` is null.

`backgroundWorkHoldsCompletion` parks the runtime at idle so "session idle" still presents, except a failed run stays visible. A leftover command such as a dev server does not hold completion (comment citing issue 14872).

`delegate_task` requires an active parent run owned by this MCP session, else `parent_not_active`. Completion wake: mode `wait` uses `settled_only` (the blocking call owns delivery; wake only if the parent settled first via timeout or disconnect); otherwise `always`. `waitForTask` polls with sleep and a timeout, then best-effort sets a wake policy so a later terminal still wakes a mid-turn parent.

If `task.result` is null but progress says `result_available` and the result is pending across a restart, work state is forced back to `working`. A cut continuation is not reported done.

Failure tags in `threadManagementFailure`: `thread_not_found`, `run_not_found`, `thread_not_sendable` (archived, or no steerable run), `thread_not_interruptible`, `orchestration_error`.

Copy: do not report a child done while a restart continuation is in flight. Do not hold "finished" open because a dev server is still running. The parent-not-active refusal is the right analogue of herdr's `agent_not_ready`.

### claude-squad — scrape and auto-yes

**VERIFIED-IN-SOURCE** `session/instance.go`, `session/tmux/tmux.go`.

Statuses: `Running`, `Ready`, `Loading`, `Paused`. `Start` sets `Running` as soon as tmux is up. There is no hook and no activity gate. `AutoYes` calls `TapEnter`, which writes `0x0D` to the tmux PTY. `CheckAndHandleTrustPrompt` captures the pane and presses Enter on Claude's "Do you trust the files in this folder?" or "new MCP server", or `D` plus Enter on aider's docs prompt.

On restore, a missing tmux session (`ErrSessionNotFound`) pauses the instance instead of failing the whole load, so one dead session does not hide the others. The worktree and branch stay on disk.

Avoid the scrape and the blind Enter. The pause-on-missing-session behavior is worth copying for crash recovery.

### Claude Code and Codex hooks

**FROM-DOCS**, Claude hooks page.

`Stop` runs when the main agent has finished responding. It does not run on a user interrupt. API errors fire `StopFailure`. Input adds `stop_hook_active`, `last_assistant_message`, `background_tasks`, `session_crons`. `background_tasks[].type` includes `shell`, `subagent`, `monitor`, `workflow`, `teammate`. After eight consecutive Stop continuations, Claude ends the turn anyway (`CLAUDE_CODE_STOP_HOOK_BLOCK_CAP` raises it).

`decision: "block"` on Stop prevents the stop and feeds `reason` back. `hookSpecificOutput.additionalContext` continues the conversation as hook feedback rather than an error. Exit 2 does the same as `reason`.

`Notification` matchers include `permission_prompt` (about 6s after the prompt appears, and each keystroke defers it), `idle_prompt` (about 60s after the reply, only if you have not typed and no background agent is running), `agent_needs_input`, `agent_completed`. The page says to use `PermissionRequest` when the hook must run immediately. `idle_prompt` is a desktop-notification timer, not turn completion.

`WorktreeCreate` replaces default git for `--worktree`, `isolation: "worktree"`, and background sessions. A command hook prints the worktree path on stdout. Any non-zero exit fails creation. It cannot return JSON, because stdout is the path. `WorktreeRemove`: any non-zero exit fails removal if the directory still exists; JSON is discarded.

`claude -p` (**FROM-DOCS**, headless page): exit 0 on success, non-zero on failure. `--output-format text` (default), `json` (`result`, session id, metadata), `stream-json` (last line is a `result` message). `--resume <session_id>` continues. Async hooks are killed at teardown (hooks page). A `PreToolUse` `permissionDecision: "defer"` is honored only with `-p`.

**FROM-DOCS**, Codex hooks page.

Common stdin: `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `model`, and `turn_id` on turn-scoped events, plus `permission_mode`. Default hook timeout is 600s. `Interrupt` defaults to 1s and configured timeouts are clamped to 1–3s. `Stop` must print JSON on exit 0; plain text is invalid. `decision: "block"` does not reject the turn; it continues and uses `reason` as a new user prompt. `continue: false` from any matching Stop hook wins over other hooks' continuation. `SubagentStop` `continue: false` is parsed but does not stop the subagent. Hooks are trust-gated by hash; `--dangerously-bypass-hook-trust` exists (also recorded in signaltty `docs/07-agents.md`).

`notify` (**FROM-DOCS**, config-advanced): one event today, `agent-turn-complete`. The program gets one JSON argv: `type`, `thread-id`, `turn-id`, `cwd`, `input-messages`, `last-assistant-message`. This is the side channel signaltty already prefers over a Codex Notification hook, which the hooks page does not list.

`codex exec` (**FROM-DOCS**, non-interactive mode): progress on stderr, final assistant message on stdout. `--json` switches stdout to JSONL: `thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.*`, `error`. `--output-last-message` writes the final text and still prints it. `--ephemeral` skips rollout files.

Signaltty `docs/07-agents.md` says Codex 0.159.1 honors configured timeouts without a 3s clamp, and that interactive launches need `--no-daemon` because the daemon can steal `SIGNALTTY_PANE`. Treat that as this project's measurement of a specific binary. The public hooks page still documents the Interrupt 1–3s clamp and a 600s default. Do not collapse those into one rule.

---

## C. Parent, child, labels, and the result channel

### t3code lineage and delivery

**VERIFIED-IN-SOURCE** `packages/contracts/src/orchestrationV2.ts`.

`OrchestrationV2ThreadShell` includes `id`, `projectId`, `title`, provider and model, `branch`, `worktreePath`, PR links, `lineage`, `forkedFrom`, active provider thread, run pointers, `activityRunStatus` (`preparing | starting | running | waiting`), `status`, errors, pending runtime request, pending background tasks, archive and settle timestamps, snooze, pin.

Lineage: `parentThreadId`, `relationshipToParent` of `fork | subagent | null`, `rootThreadId`. Context-transfer types include `fork`, `provider_handoff`, `merge_back`, `subagent_spawn`, `subagent_result`.

`isProviderNativeSubagentThread` is `relationshipToParent === "subagent"` and `creationSource === "provider"`. The comment says a subagent the provider spawned itself (Claude Agent tool, Codex/Cursor native subagents) cannot take messages. A `delegate_task` child (`creationSource: "mcp"`) can. Creation sources: `web`, `mobile`, `mcp`, `provider`, `server`.

Delegated completion delivery: `pending`, `claimed`, `acknowledged`, `delivered`, `disposed`. Cohort disposition: `open | stopped | disposed` with a generation, message id, and task ids. Restart-cancelled background work (`subagent | shell | monitor | task`, plus label and optional stable id) is told to the next provider turn once.

`readTask` derives status from `task.result` and the child run (`completed | failed | cancelled | interrupted`, else queued vs running), summary text from assistant or error turn items, and finds `subagent_result` transfers from child to parent. A terminal status is acknowledged with `observedByRunId` when the observing run's provider matches.

Copy: explicit lineage, the provider-native vs app-owned split, the delivery state machine, and `subagent_result` as a typed payload (status, summary, child id, terminal run). The MCP assumption does not transfer: signaltty's parent is a pane, not an in-harness MCP client. Use the payload, deliver it the cmux way.

### cmux agent.message

**VERIFIED-IN-SOURCE** `Packages/macOS/CmuxAgentJournal/.../AgentMessage.swift`, `AgentMessagePromptRenderer.swift`, `CLI/CMUXCLI+AgentMessages.swift`.

Fields: `id`, `threadId`, `senderName`, `senderSurfaceId`, `senderWorkspaceId`, `recipientSurfaceId`, `recipientWorkspaceId`, `body`, `createdAt`, `inReplyTo`, `state` (`queued → delivered → read`, or `queued → failed`), `deliveredVia`, `readAt`, `failureReason`.

Limits: body 32KiB, sender name 64, reject C0 controls except newline and tab. States only move forward. An already-delivered message cannot fail. Missing surface or disabled hooks fail open by printing `{}`.

Delivery is hooks only, so a message cannot land in a human's draft. The rendered block is:

- header `[cmux agent message] from NAME`
- `Message id`, optional `In reply to`
- a disclaimer
- `Reply with: cmux agent message --reply-to ID`
- body between `---` fences that include the id, so the body cannot fake the end marker

Claude `SessionStart` / `Stop`: an async rewake polls `agent.message.poll` every 2s. If something is queued, claim with `defer_delivery`, write the text to stderr, ack, exit 2. That wakes Claude as a system reminder and does not touch the prompt box. `UserPromptSubmit` drains the inbox into `hookSpecificOutput.additionalContext`.

Codex cannot be woken once idle. The Codex `Stop` inbox hook returns `decision: block` plus the reason text, which continues the turn, and marks the message delivered/read.

Headless shares the surface id but must not claim. The env gates are `CMUX_CLAUDE_HEADLESS` and `CMUX_CODEX_HEADLESS` equal to `1`.

Journal kinds `childSpawned`, `childCompleted`, `childFailed`, `messagePublished` do not change the pane lifecycle phase.

Human labels: surfaces have names; the message carries `senderName`. This is not a separate label table.

Copy this channel almost as specified, including fail-open, forward-only states, the fenced id, the headless exclusion, and the Codex `Stop` + `decision: block` wake. A bare `signaltty report --result` event is weaker if the parent only learns by polling. Emit the event and also inject it on the parent's next `Stop` or `UserPromptSubmit`.

### herdr, workmux, claude-squad

herdr: human name is `agent.start` `name` plus `agent.rename` (**VERIFIED-IN-SOURCE** schema). No parent pointer and no result payload (search of `src` above). The parent waits on status and reads the screen.

workmux: the handle name is the human label. `agent_session_id` binds hook events that have no pane id, by process ancestry (**VERIFIED-IN-SOURCE** `src/state/types.rs`). No structured result.

claude-squad: `Instance.Title` is the human label and becomes the branch suffix. No parent, no result. Instances are a flat list in `session/storage.go`.

### What signaltty should store

**INFERRED** from the three models that have a relation at all:

- `parent_pane_id` and `root_pane_id`, set at spawn, not inferred later from `/proc` alone. Ancestry is a fallback for hook attribution (workmux and signaltty `docs/07` already do that).
- `relationship`: `fork` or `subagent`. Provider-native subagents do not move the parent pane's lifecycle (cmux and t3code both do this).
- A label string set at creation.
- A result record: id, child id, status, summary text, optional detail, state `queued | claimed | delivered | read | failed`, created time. Cap the body. Fence it. Claim is single-flight. Headless clients do not claim the interactive pane's inbox.

---

## D. The task that outlives the pane

### t3code — thread is the task

**VERIFIED-IN-SOURCE** `packages/contracts/src/orchestrationV2.ts` (thread shell, run), `apps/server/src/vcs/GitVcsDriver.ts`, `apps/server/src/vcs/GitVcsDriverCore.ts` `createWorktree`, `apps/server/src/storageCleanup.ts`.

Create: `targetBranch = newRefName ?? refName`. Default path is `worktreesDir/repoBasename/sanitizedBranch` (slashes become dashes). Command is `git -c checkout.workers=N worktree add [-b newRef] path ref`, with `LC_ALL=C`. `onWorktreeClaimed` fires once `git worktree add` has registered the directory and before submodule update. The comment says that path is safe to remove on cancel because git refuses an existing path. Submodule update is best-effort and must not roll the thread back. A missing checkout is "git worktree add failed".

Scheduled tasks: bound to a thread uses workspace strategy `root`. Unbound launches a fresh worktree per run `{ type: worktree, baseRef: "main", startFromOrigin: true }` (`OrchestratorMcpService.ts`).

Cleanup on thread delete skips the checkout when:

- the effect outbox has rows that are not succeeded or cancelled
- any non-stopped provider session has cwd inside the worktree
- status shows working tree changes, a branch mismatch, HEAD moved, or ignored files other than `node_modules/`

Then `git worktree remove` with `force: false`. The comment says to preserve branch and path because `ProviderTurnStartService` recreates the checkout from that branch on resume. `deleteLocalBranch` and `pruneWorktrees` are separate driver methods. Dirty detection is porcelain, not English stderr.

`OrchestrationV2Run` fields include id, thread id, ordinal, provider, model, provider thread id, user message id, status, queue position, `queueHeld` (restart recovery holds the queue until an explicit resume), timestamps, checkpoint id, `restartContinuationOfRunId`, and `restartCancelledBackgroundWork`.

Copy: the thread outlives the runtime; keep the branch so resume can recreate the checkout; porcelain dirty check; do not remove while a session or an outbox item is live; do not roll back because submodule init failed; claim the path before slow follow-up work. Diff against the recorded baseline (checkpoint ordinal 0), not against `HEAD`.

### workmux — merge and discard

**VERIFIED-IN-SOURCE** `src/git/worktree.rs` `create_worktree_in`, `src/workflow/merge.rs`.

Create: `git worktree add [-b branch] path [base]`. If `create_branch` and not tracking upstream, upstream is unset. `worktree_exists` maps branch to path.

Merge (`workflow/merge.rs`):

- If the source will be deleted (`!keep`) and it has unstaged or untracked changes, and `--ignore-uncommitted` is not set, refuse. Those changes would be lost. Staged changes are committed with git's editor unless ignored.
- Refuse merging a branch into itself.
- Refuse if the target worktree has uncommitted tracked changes. Untracked files on the target are allowed; git fails later if they collide.
- Optional `pre_merge` hooks get `WORKMUX_HANDLE`, `WM_BRANCH_NAME`, `WM_TARGET_BRANCH`, `WM_WORKTREE_PATH`, `WM_PROJECT_ROOT`.
- Rebase conflicts stay in the source worktree. The user runs `git rebase --continue` or `--abort`.
- Squash and merge conflicts abort or reset the target so the target stays clean, then the error tells the user to rebase or merge inside the source and retry.
- After success, unless `--keep`, cleanup uses `force: true`, `keep_branch: false` (the branch is deleted).
- If cleanup fails, the merge result is still `Ok` and `cleanup_error` is set. The merge is not reported as failed.

This is the finish policy to copy. `task finish --merge` and `task finish --discard` should be explicit and separate from `worktree.remove`.

`wait`'s "path gone = merged, path still there = crash" only makes sense once the task, not the pane, owns the worktree.

### vibe-kanban — durable row, aggressive cleanup

**VERIFIED-IN-SOURCE** `crates/db/src/models/task.rs`, `crates/db/migrations/20250617183714_init.sql`, `crates/worktree-manager/src/worktree_manager.rs`, `crates/git/src/lib.rs`, `crates/git/src/cli.rs`.

Task row: `id`, `project_id`, `title`, `description`, `status`, `parent_workspace_id`, timestamps. Status check constraint: `todo`, `inprogress`, `inreview`, `done`, `cancelled`. The Rust enum is `Todo | InProgress | InReview | Done | Cancelled`. `task.rs` in this clone only has `find_all` and `find_by_id`. The writer that moves a card to Done was not found there.

**FROM-DOCS** `docs/core-features/completing-a-task.mdx`: Merge moves the task to Done. The branch stays until a manual delete. A GitHub PR merge also moves the task to Done. Rebase conflicts are a visible status; the agent can be handed generated resolution instructions, or the user aborts (`docs/core-features/resolving-rebase-conflicts.mdx`).

Worktree manager: a global `tokio` mutex per path so concurrent creates do not race. `create_worktree` creates the branch from `base_branch`, then `ensure_worktree_exists`. If the worktree is not "properly set up" it is recreated. On `git worktree add` failure it force-cleans metadata and the directory and retries once.

`cleanup_worktree` takes the same lock, then `git worktree remove --force`, deletes the git metadata dir, `remove_dir_all` on the path, and `git worktree prune`. Errors from `git worktree remove` are logged and not fatal. `GitService::remove_worktree` and `delete_branch` are separate. `reset` to a commit checks the tree is clean unless `force`, and `force` also runs `git clean -fd`. Conflicted files come from `git diff --diff-filter=U`.

Copy: the task row outliving the process, the per-path lock, porcelain dirty checks, conflicted-file listing, and "branch stays until an explicit delete". Avoid: force `remove_dir_all` as the default finish. That is an orphan-recovery tool, not `task finish`.

Executors are processes. Completion of an attempt is the process exit plus log normalization (`crates/executors/src/executors/codex/normalize_logs.rs` records `exit_code`). That is a different "done" from the kanban column. **INFERRED**: the column does not move just because the process exited, because the docs require an explicit merge or a merged PR. The status-write call itself was not located.

### claude-squad — session is the task, kill is destructive

**VERIFIED-IN-SOURCE** `session/git/worktree.go`, `session/git/worktree_ops.go`, `session/git/worktree_git.go`, `session/git/diff.go`, `session/instance.go`.

Fields: `Title`, `Path`, `Branch`, `Status`, `Program`, `AutoYes`, `Prompt`, `CreatedAt`, `UpdatedAt`, plus worktree data (`RepoPath`, `WorktreePath`, `BranchName`, `BaseCommitSHA`, `IsExistingBranch`) and diff stats.

Setup of a new session: path is under the config dir `worktrees/`, sanitized branch name plus a nanosecond suffix. Branch is `BranchPrefix + sessionName`. It records `HEAD` of the current checkout as `baseCommitSHA` (there is a TODO about using main/master instead). It `git branch -D`s any existing same-named branch, then `git worktree add -b branch path <that commit>`, so uncommitted changes in the source tree are not inherited. An empty repo with no HEAD fails with a message to make an initial commit. An existing branch is not deleted on cleanup (`isExistingBranch`).

`Cleanup` (used by `Kill`): `git worktree remove -f`, then `git branch -D` unless the branch pre-existed, then `worktree prune`. `Remove` drops the worktree and keeps the branch (pause). `IsDirty` is `git status --porcelain`. `IsValidWorktree` is "path exists and has a `.git` entry". `PushChanges` stages everything, commits with `--no-verify`, and pushes. There is no merge-into-base.

Diff: `git add -N .` then `git diff <baseCommitSHA>` (and `--numstat` for the list). The comment says an unset base SHA makes every diff fail with an ambiguous argument. Unselected instances get numstat only.

Crash: restore of a dead tmux server pauses the instance and leaves the worktree.

Copy: record the base commit at create and diff against it; include untracked files via intent-to-add; pause rather than destroy when the terminal session is gone; do not delete a branch the user already had. Avoid: `branch -D` on kill, `--no-verify` commits, basing the branch on whatever HEAD the source checkout happens to be on, and `git add -N` as a side effect of a read-only diff (it changes the index).

### herdr worktrees

**VERIFIED-IN-SOURCE** `src/worktree.rs`, `src/api/schema/worktrees.rs`.

Add: `git worktree add -b branch path base`, or `add path branch` when `refs/heads/<branch>` already exists. Remove: `git worktree remove [--force] path` with `LC_ALL=C`. No `-b` branch-delete flag in the command builder. A search of `src` for `branch -d` / `branch -D` found nothing.

Dirty detection matches English stderr: `"contains modified or untracked files"` plus `"use --force to delete it"`, or the submodule sentence `"working trees containing submodules cannot be moved or removed"`. Force plus "is not a working tree" may delete a leftover directory only if `worktree list` no longer contains the path and the leftover checkout matches the repo.

Params include `workspace_id` / cwd, `branch`, `base`, `path`, `label`, `focus`, `trust_repository`. The workspace is the unit. There is no task object and no merge-on-finish in this command layer.

Copy: `LC_ALL=C`, the leftover-directory guard. Avoid: classifying dirty by English git text. Porcelain does not depend on locale. herdr sets `LC_ALL=C` and still matches prose; a git wording change breaks it.

### Conductor

**FROM-DOCS**. Closed source. Pages: `docs/concepts/workspaces-and-branches`, `docs/concepts/git-worktrees`, `docs/concepts/workflow`, `docs/concepts/parallel-agents`, `docs/guides/review-and-merge`, `docs/core/conductor-json`, `docs/reference/files-to-copy`, `docs/guides/git-worktrees/run-claude-code-with-git-worktrees`.

- A workspace is one branch plus one working tree plus a running environment. Git allows a branch in one worktree at a time, and Conductor keeps that.
- New workspace: fetch `origin`, branch from the configured base (example `origin/main`), so a stale local checkout is not the base. The first chat instructs the agent to rename the branch to match the work.
- New worktrees contain tracked files only. Gitignored files (`.env`, local databases) are copied by a "files to copy" / `.worktreeinclude` list, or produced by `scripts.setup`. `scripts.run` starts the app. `scripts.archive` runs on archive. `runScriptMode: "nonconcurrent"` kills an in-progress run script before the next one.
- Workspaces live in `~/conductor/workspaces/` (since 0.25.0, December 2025), not inside the repo.
- Several agents may share one workspace when they must share the branch. Independent work gets independent workspaces.
- Review is a diff viewer. Checks gather git status, PR metadata, CI, deployments, comments, and todos. Conflicts: "ask an agent to help resolve them", then rerun tests. No automatic merge-conflict state machine is documented.
- After merge, archive removes the workspace from the active list. Restore from History brings the workspace back, including chat history. The docs do not say that archive deletes the branch or the worktree.

Copy: fetch the base before creating the branch; one workspace per shippable unit; setup/archive scripts; keep chat attached to the workspace across archive. Do not invent a cleanup policy the docs do not state.

---

## Failure modes

| Failure | Who handles it | How | Copy? |
| --- | --- | --- | --- |
| Agent not ready / still launching | herdr | `agent_not_ready`, no bytes written | Yes |
| Agent blocked on a permission | herdr, cmux | herdr refuses the prompt. cmux `needsInput` outranks idle but loses to `running` if another session is live | Yes. Do not type the prompt into the permission UI |
| Prompt lost (typed into a busy or half-open UI) | herdr | 5s activity gate, then `agent_prompt_stalled` | Yes. workmux does not do this |
| Draft already on screen | cmux | `input_state.draft` / `dialog` blocks typing unless forced. Enter-only is allowed | Yes |
| Turn finished vs idle vs needs-user | hooks | Claude `Stop` vs `PermissionRequest`. Codex `Stop` vs `PermissionRequest` vs `notify` `agent-turn-complete`. `idle_prompt` is 60s and the wrong signal | Hooks. Not the notification timer |
| Subagent stop looks like parent done | workmux, cmux, t3code | `SubagentStop` sets working, not done. cmux ignores `isSubagent` for the badge. t3code provider-native subagents are not steerable and are not the parent | Yes |
| Background shell still running | t3code, Claude `Stop` | Dev server does not hold completion. Claude passes `background_tasks` so a hook can tell "paused for background work" from "done" | Yes |
| Crash / restart mid-turn | t3code, claude-squad, workmux | t3code holds the result and the queue until resume. claude-squad pauses when tmux is gone and keeps the worktree. workmux `boot_id` + `pane_pid` + `command` tell crash from pane-id reuse | Yes |
| Restart reports the child done too early | t3code | If result is pending across restart, force work state back to working | Yes |
| Orphaned worktree | t3code, vibe-kanban, herdr, claude-squad | t3code will not remove while a session cwd is inside. vibe-kanban force-removes metadata and the directory under a lock. herdr deletes a leftover dir only after git says it is not a worktree and the checkout still matches. claude-squad `IsValidWorktree` is path plus `.git` | Copy t3code's skip and herdr's leftover guard. Use vibe-kanban's force path only as explicit recovery |
| Dirty tree on finish | workmux, t3code, herdr, vibe-kanban | workmux refuses unstaged/untracked on a source it will delete, and refuses a dirty target. t3code skips removal on porcelain dirt. herdr matches English stderr. vibe-kanban `check_worktree_clean` unless `force` | Porcelain, workmux's refuse rules. Not English stderr |
| Merge conflict | workmux, vibe-kanban, Conductor | workmux: rebase conflicts stay in the source; squash/merge conflicts reset the target. vibe-kanban: list `diff-filter=U`, hand the agent a prompt, or abort. Conductor: ask the agent, no documented auto-resolve | workmux's "target stays clean" |
| Cleanup fails after merge succeeds | workmux | `Ok` plus `cleanup_error`. The merge is not rolled back in the report | Yes |
| Branch deleted too early | claude-squad vs everyone else | claude-squad `branch -D` on kill. t3code, herdr, vibe-kanban docs, and workmux-until-successful-merge keep the branch | Delete the branch only after a successful merge, and never if the user supplied an existing branch |
| Submodule init fails | t3code | Logged, worktree kept | Yes |
| Concurrent create of the same path | vibe-kanban | Per-path mutex, one retry after metadata cleanup | Yes for the lock. The retry should not `remove_dir_all` a path it did not create |
| Headless run steals the interactive inbox | cmux | `CMUX_CLAUDE_HEADLESS` / `CMUX_CODEX_HEADLESS` must not claim | Yes |
| Codex idle cannot be poked | cmux | Deliver by `Stop` `decision: block`, which continues the turn | Yes, for Codex. Claude can be woken with exit 2 from SessionStart/Stop without typing |
| Diff baseline wrong | claude-squad, t3code, signaltty today | claude-squad stores the setup commit. t3code diffs checkpoint 0. signaltty `workspace.diff` is worktree vs HEAD (**FROM-DOCS** `docs/08-ipc.md`) | Store the base commit at task create. Diff against that |

---

## Recommendations for signaltty

Ranked. Each item says what to copy and what to leave.

1. **Submit like herdr, not like a raw `pane.input`.** Bracketed-paste the prompt, then send Enter as a separate key after a short delay (herdr uses 300ms). Refuse before any write when the agent is blocked, not in the foreground, or not `interactive_ready`. Pair the submit with the existing `wait.after` baseline: require a new working or blocked transition within about 5s (`agent_prompt_stalled`), then wait for the settled state. Replay events from the pre-submit sequence so the activity gate does not eat the completion. Keep the Windows-Codex "Right after paste", Copilot focus-gained, and workmux's 50ms bang-delay as a profile table, not as the default path.

2. **Make the child result a claimable inbox, and put a structured payload in it.** Copy cmux `agent.message`: durable `queued → delivered → read`, forward-only, body cap, C0 rejected, fenced with the id, fail-open when hooks are missing, never keystrokes. Copy t3code's payload: status, summary text, child id, terminal run, delivery states through acknowledged. Inject on the parent's `UserPromptSubmit` as `additionalContext`, and on Claude wake with exit 2 from `Stop`/`SessionStart` so the prompt box stays untouched. For Codex, deliver a queued result by `Stop` returning `decision: "block"` with the text as `reason`, because Codex will not wake from idle. Headless `claude -p` and `codex exec` must not claim the interactive pane's inbox. A `signaltty report --result` event is the write side of this inbox, not a substitute for injection.

3. **Hooks decide ready, blocked, and done. The screen does not.** Map like workmux: `UserPromptSubmit` and `PostToolUse` → working, `PermissionRequest` (and Claude `Notification` `permission_prompt` only as a late backup) → blocked, `Stop` → done. `SubagentStop` stays working. Do not use Claude `idle_prompt` (60s, only when the user looks away). Do not use cmux hibernation or workmux's focus-clear. Screen and OSC stay the fallback, with herdr's debounce (100ms, 3 confirmations, 700ms cap, 3s startup grace) and a startup grace so a fresh pane is not "done". Codex `notify` `agent-turn-complete` remains the side channel it already is; `Stop` stdout must be JSON (public Codex docs: plain text is invalid).

4. **Read rendered text, and add the cursor nobody else has.** herdr, cmux, workmux, and claude-squad all return a window of rendered lines. herdr's `revision` is hardcoded 0. Signaltty should return a monotonic content sequence plus a line or byte offset into the rendered transcript, and say when scrollback has dropped the requested range. Do not use raw `pty.data` / `output_offset` as the agent read. Alt-screen: if the caller needs history and the agent is working, return not-idle instead of scrolling it (herdr). Put "only what's new" for structured results on an event sequence (`after_seq`), the way t3code uses `snapshotSequence`. Do not parse `~/.claude/projects/*.jsonl` or Codex rollout files as the primary channel; both docs say the file lags or is unstable. `last_assistant_message` on `Stop` is the supported text.

5. **A task row outlives the pane. The worktree exists before the agent.** Fields: prompt, agent, label, parent task or pane, branch, worktree path, base commit SHA, state, result id. Create fetches or records the base, then `git worktree add -b` under a per-path lock (vibe-kanban), and only then spawns the pane and submits. `wait` must be able to start before the agent process exists (workmux). Status for "who needs the user" is the hook state of the panes, ranked like herdr (blocked, then unseen done, then working, then seen idle) and combined like cmux when several sessions share a surface (running beats needs-input beats idle). Persist enough to tell crash from recycle: pid, foreground command, mux boot id, agent session id (workmux `AgentState`).

6. **Finish is an explicit merge or discard, not `worktree.remove`.** Copy workmux: refuse unstaged and untracked changes on a worktree you are about to delete unless the caller passes an explicit ignore; do not invent a commit message for staged work; refuse a dirty target; on conflict, reset the target so it stays clean and leave the resolution in the source worktree; delete the branch only after the merge succeeds, and never delete a branch the user already had (claude-squad `isExistingBranch`). If cleanup fails after the merge landed, report the cleanup error separately. Discard force-removes only that task's worktree. Copy t3code's skips: do not remove while a live session's cwd is inside, while an outbox or result delivery is unfinished, or when porcelain shows dirt, a branch mismatch, or a moved HEAD. Keep the branch so a resume can recreate the checkout. Detect dirt with porcelain, not herdr's English stderr match. Do not roll the worktree back because submodule init failed. Diff the task against the base commit stored at create (claude-squad, t3code checkpoint 0). `workspace.diff` against HEAD is the wrong baseline. `git add -N` must not be a side effect of computing that diff.

7. **Parent and child are written down at spawn.** `parent_pane_id`, `root_pane_id`, relationship `fork | subagent`, label. Provider-native subagents (Claude Agent tool, Codex subagents) do not move the parent pane's lifecycle and are not steerable. App-owned children are. `/proc` ancestry stays a fallback for hook attribution when the hook has no pane id, which workmux and signaltty already need.

8. **Do not copy these, even where they look convenient.** claude-squad auto-yes and trust-prompt scraping. claude-squad `git branch -D` on kill and commit `--no-verify`. workmux send-into-a-busy-agent and the 2s status poll as a lost-prompt detector. cmux `waiting_on_human` as the only ready bit. vibe-kanban's default `remove_dir_all` plus force worktree remove. Basing the new branch on whatever HEAD the source checkout has, instead of a fetched configured base (Conductor) or an explicit base ref (t3code scheduled tasks use `main` with `startFromOrigin`). Treating a vanished worktree as success unless a task record says the merge completed (workmux's wait does this; it is only safe after recommendation 6).

## Gaps left open

- Conductor archive does not document worktree or branch deletion. Closed source, so that stays unknown.
- vibe-kanban's function that writes `TaskStatus::Done` was not in `crates/db/src/models/task.rs` (finders only). The docs describe the transition; the call site was not confirmed.
- t3code `ProviderTurnStartService` was not opened. The cleanup comment says it recreates the checkout from the saved branch. That comment is **VERIFIED-IN-SOURCE**; the recreate function body is not.
- cmux issue 8950 asked for an agent `wait` verb in 2026-07. This clone's CLI `wait` cases are vm and browser only. Whether some other binary command wraps lifecycle was not exhaustively searched past those dispatch tables.
- Codex `exec` exit codes beyond "error vs success" were not tabulated. The docs that were read specify stream shapes, not a full exit-code table.
