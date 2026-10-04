# State of the Art: Multi-Agent Orchestration Practices (Feature 018)

**Date**: 2026-10-04 · **Scope**: what the agents/frameworks/protocols themselves do (primary sources).
Companion docs: `research-codebase.md` (what signaltty has), plus the parallel source-dive on
t3code/cmux/herdr/workmux/Conductor/Vibe Kanban/Claude Squad (not duplicated here).

**Claim tags**: `[VERIFIED <url>]` = fetched + read in full during this research (Oct 2026).
`[FROM-DOCS <url>]` = official doc located via search, snippet-confirmed, not full-fetched.
`[INFERRED]` = synthesis, no single source. Dates given for time-sensitive facts.

Context: signaltty = native Linux (Rust/GTK4) workspace; JSONL-over-Unix-socket server owns
PTYs/state; GUI + `signaltty` CLI are clients; agents run in real terminal panes
[FROM-DOCS /home/simone/code/signaltty/AGENTS.md]. Feature 018: an in-pane agent orchestrates
agents in other panes (spawn in isolated worktrees, reliable prompt delivery, readiness/turn
detection, output reading, structured results, review → merge/discard).

---

## 1. Claude Code (Anthropic)

### 1.1 Hooks — the readiness/completion substrate

Claude Code fires the same hook events in terminal, IDE, Desktop app and cloud sessions; handlers
can be shell commands, HTTP endpoints, MCP tool calls, LLM prompts or subagents
[VERIFIED https://code.claude.com/docs/en/hooks].
Full event list (current): `SessionStart`, `Setup`, `UserPromptSubmit`, `UserPromptExpansion`,
`PreToolUse`, `PermissionRequest`, `PermissionDenied`, `PostToolUse`, `PostToolUseFailure`,
`PostToolBatch`, `Notification`, `MessageDisplay`, `SubagentStart`, `SubagentStop`, `TaskCreated`,
`TaskCompleted`, `Stop`, `StopFailure`, `TeammateIdle`, `InstructionsLoaded`, `ConfigChange`,
`CwdChanged`, `DirectoryAdded`, `FileChanged`, `WorktreeCreate`, `WorktreeRemove`, `PreCompact`,
`PostCompact`, `PreModelSwitch`, `PostModelSwitch`, `Elicitation`, `ElicitationResult`,
`SessionEnd` [VERIFIED https://code.claude.com/docs/en/hooks].
Cadences: per-session (`SessionStart`/`SessionEnd`), per-turn (`UserPromptSubmit`/`Stop`/
`StopFailure`), per-tool-call (`PreToolUse`/`PostToolUse`, except `EndConversation`)
[VERIFIED https://code.claude.com/docs/en/hooks].

Key details for an orchestrator:
- `Notification` matcher values include `permission_prompt`, `idle_prompt`, `auth_success`,
  `elicitation_dialog`, `elicitation_url_dialog`, `elicitation_complete`, `elicitation_response`,
  `agent_needs_input`, `agent_completed`, `quota_auto_resume_*` — i.e. idle/completion arrivals
  are first-class hook signals, not screen state [VERIFIED https://code.claude.com/docs/en/hooks].
- Matchers: `*`/empty = all; `[A-Za-z0-9_ |,-]` = exact or `|`/`,`-separated list; anything else
  = unanchored JS regex [VERIFIED https://code.claude.com/docs/en/hooks].
- Hook scopes merge (user + project + local + managed + plugin + skill frontmatter + subagent
  frontmatter); hooks from settings/plugins also fire inside subagents, with `agent_id`/
  `agent_type` in input [VERIFIED https://code.claude.com/docs/en/hooks].
- `SessionStart` matchers: `startup|resume|clear|compact|fork` — supports context re-injection
  after compaction (`compact` matcher) [VERIFIED https://code.claude.com/docs/en/hooks].
- `WorktreeCreate`/`WorktreeRemove` hooks replace default git behavior (custom VCS support);
  `Stop` = turn finished; `SubagentStop` = worker finished; `TeammateIdle` = team member about
  to idle [VERIFIED https://code.claude.com/docs/en/hooks].
- `PermissionRequest` returns decisions via `hookSpecificOutput` (`behavior: allow/deny`);
  signaltty already bridges this natively with a 120s server wait vs 125s provider timeout
  [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].

Lesson: **turn-completion and idleness are hook events everywhere** (`Stop`, `SubagentStop`,
`Notification:idle_prompt`/`agent_completed`, `TeammateIdle`). Screen scraping is not part of
Anthropic's own orchestration story [INFERRED].

### 1.2 Subagents — context isolation primitive, not concurrency primitive

Subagents run in their own context window with custom system prompt, tool access, independent
permissions; usage counts against the same plan limits as the main conversation
[VERIFIED https://code.claude.com/docs/en/sub-agents].
- Built-ins: `Explore` (read-only, skips CLAUDE.md + git snapshot for speed; thoroughness
  quick/medium/very-thorough), `Plan` (plan-mode research), `general-purpose` (full tools),
  plus `claude` (catch-all, default for background sessions), `statusline-setup`, `claude-code-guide`
  [VERIFIED https://code.claude.com/docs/en/sub-agents].
- Delegation is description-driven: "write a clear description so Claude knows when to use it";
  combined non-builtin descriptions > 15,000 tokens trigger a startup warning
  [VERIFIED https://code.claude.com/docs/en/sub-agents].
- Definition = Markdown + YAML frontmatter (`name`, `description`, `tools`, `disallowedTools`,
  `model`, `permissionMode`, `mcpServers`, `hooks`, `maxTurns`, `skills`, `initialPrompt`,
  `memory`, `effort`, `background`, `omitClaudeMd`, `isolation`); scopes: managed > `--agents`
  CLI JSON > project `.claude/agents/` > user `~/.claude/agents/` > plugin
  [VERIFIED https://code.claude.com/docs/en/sub-agents].
- `--agents` also accepts a JSON file path in `-p` mode (v2.1.281+, Oct-2026 era)
  [VERIFIED https://code.claude.com/docs/en/sub-agents].
- Cost control by model routing: faster/cheaper models (Haiku) for exploration; env
  `CLAUDE_CODE_SUBAGENT_MODEL` (+ `_FORCE=1`) pins worker models
  [VERIFIED https://code.claude.com/docs/en/sub-agents].
- Subagents can spawn nested subagents; can run foreground or background
  [FROM-DOCS https://code.claude.com/docs/en/sub-agents].

### 1.3 Agent teams — Anthropic's own orchestrator pattern (experimental)

"Coordinate multiple Claude Code instances working together as a team, with shared tasks,
inter-agent messaging, and centralized management"; disabled by default, enabled via
`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`; needs v2.1.32+
[VERIFIED https://code.claude.com/docs/en/agent-teams].
- Topology: one lead session assigns tasks + synthesizes; teammates work independently in own
  contexts, share a task list, message each other directly via mailbox (not routed via lead);
  human can message any teammate directly [VERIFIED https://code.claude.com/docs/en/agent-teams].
- Subagents vs teams: subagents report back to caller (lower tokens, summarized result);
  teams self-coordinate (higher tokens, each a full Claude instance). Teams add coordination
  overhead + "significantly more tokens"; best when teammates operate independently; for
  sequential/same-file/dependency-heavy work, single session or subagents win
  [VERIFIED https://code.claude.com/docs/en/agent-teams].
- Display: `in-process` (agent panel, ↑/↓ + Enter to inspect/message; idle rows hide 30s after
  all-idle, collapse beyond 3) or split panes via tmux / iTerm2+`it2` CLI (`teammateMode`:
  `in-process|auto|tmux|iterm2`; `--teammate-mode` flag)
  [VERIFIED https://code.claude.com/docs/en/agent-teams].
- Model pick order per teammate: spawn prompt > subagent-def `model` (`inherit` = lead's) >
  `CLAUDE_CODE_SUBAGENT_MODEL` > lead's model; org `availableModels` allowlist enforced with
  fallback; effort level inherited (split-pane from v2.1.186)
  [VERIFIED https://code.claude.com/docs/en/agent-teams].
- Plan-before-implement: teammates spawned while lead is in plan mode stay read-only until plan
  approval (auto-approved on arrival at lead; edits still go through permission prompts)
  [VERIFIED https://code.claude.com/docs/en/agent-teams].
- Constraint: spawning teammates requires interactive session; in `-p`/SDK mode named subagents
  run as ordinary subagents even with teams enabled
  [VERIFIED https://code.claude.com/docs/en/agent-teams].

### 1.4 Headless `claude -p` — the scriptable worker interface

`claude -p/--print` = non-interactive one-shot; reads stdin, writes stdout, pipes like a shell
tool; exit 0 on success, non-zero on failure; rejects `--bg`, and `--cloud`+task-desc
[VERIFIED https://code.claude.com/docs/en/headless].
- `--output-format text|json|stream-json`; `--json-schema <schema>` yields `structured_output`
  (invalid schema = hard error since v2.1.205; `format` keyword is annotation-only)
  [VERIFIED https://code.claude.com/docs/en/headless].
- Streaming: `stream-json` + `--verbose` + `--include-partial-messages`, one JSON object/line,
  last line = `result` (final text, cost, session metadata); exit drain wait scales with queue,
  capped 30s (was ~2s before v2.1.214) [VERIFIED https://code.claude.com/docs/en/headless].
- Subagent messages carry `parent_tool_use_id` (main conversation = null); `--forward-subagent-text`
  (v2.1.211+) forwards text/thinking at every nesting depth → full tree rebuildable
  [VERIFIED https://code.claude.com/docs/en/headless].
- `--continue` / `--resume <id>` continue conversations; JSON output includes `total_cost_usd` +
  per-model breakdown, cumulative across resumed runs (client-side estimates)
  [VERIFIED https://code.claude.com/docs/en/headless].
- `--bare`: skips hooks/skills/plugins/MCP/CLAUDE.md/memory discovery (deterministic CI);
  never reads OAuth/keychain — needs `ANTHROPIC_API_KEY`; only CLI-supplied MCP connects; no
  system reminders; no background tasks (from v2.1.286) [VERIFIED https://code.claude.com/docs/en/headless].
- Lifecycle: background Bash killed ~5s after final result; background subagents/workflows hold
  `-p` open until done, capped 10 min idle (`CLAUDE_CODE_PRINT_BG_WAIT_CEILING_MS`, 0 = uncapped);
  SIGTERM → exit 143, unfinished turn, kills Bash tree, runs only `SessionEnd` hooks;
  `CLAUDE_CODE_RESUME_INTERRUPTED_TURN=1` continues interrupted turn on resume
  [VERIFIED https://code.claude.com/docs/en/headless].
- Session pinning: `--session-id <uuid>` for new pinned sessions; `--settings <file>` for hook
  injection; `claude attach/logs/stop/rm/agents` manage sessions
  [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].

### 1.5 Worktrees — file isolation with teeth

`claude --worktree/-w <name>` creates `.claude/worktrees/<name>/` on branch `worktree-<name>`
(auto name like `bright-running-fox` if omitted); `-p` skips workspace-trust check
[VERIFIED https://code.claude.com/docs/en/worktrees].
- In-session: `EnterWorktree`/`ExitWorktree` tools; entering outside `.claude/worktrees/` needs
  approval (only `bypassPermissions` skips; since v2.1.206) [VERIFIED https://code.claude.com/docs/en/worktrees].
- Cleanup on interactive exit: clean+unnamed → auto-remove worktree+branch; named or dirty →
  prompt keep/remove (prints `claude --worktree <name> --resume`); unverifiable state → prompt;
  `-p` runs never auto-clean (lock held until a later stale-lock sweep)
  [VERIFIED https://code.claude.com/docs/en/worktrees].
- Resume re-enters the worktree (interactive, `-p --continue/--resume`, SDK); transcript
  follows the cwd (v2.1.198+); deleted worktree → resume at launch dir + binding cleared
  [VERIFIED https://code.claude.com/docs/en/worktrees].
- **Four enforcement checks** (apply to session + all its subagents, interactive and background):
  (1) Edit/Write/NotebookEdit outside worktree blocked; (2) Bash/PowerShell/Monitor cwd resolving
  to main checkout blocked; (3) git redirects into main (`-C`, `--git-dir`, `GIT_DIR`,
  `GIT_WORK_TREE`, `cd`+git) blocked; (4) unparseable/dynamic command shape blocked with rewrite
  guidance — cannot be turned off [VERIFIED https://code.claude.com/docs/en/worktrees].
- `.worktreeinclude` carries gitignored files (e.g. `.env`) into new worktrees; `WorktreeCreate`/
  `WorktreeRemove` hooks allow non-git VCS [VERIFIED https://code.claude.com/docs/en/worktrees].
- Subagent worktrees branch from `origin/HEAD` by default (clean base, not your dirty tree),
  or local HEAD via `worktree.baseRef: "head"` [FROM-DOCS https://code.claude.com/docs/en/worktrees
  via search snippet; also flag `isolation:"worktree"` on Agent tool].
- Known race (open issue, Oct 2026): subagent `isolation:"worktree"` may not lock shell cwd,
  so `git switch -c` inside can move the parent repo's HEAD
  [FROM-DOCS https://github.com/anthropics/claude-code/issues/55708] — enforcement is hard;
  verify, don't assume.

### 1.6 Background sessions / agent view

`claude agents` = one screen for all background sessions (state, needs-input, done); each is a
full conversation runnable detached; `Ctrl+B` backgrounds Bash/subagents; `/tasks` = converged
panel for background bashes, cloud sessions, MCP background tasks; `bg` sessions listed in
`/resume` (May 2026, Week 21); `claude agents --json` for scripting
[FROM-DOCS https://code.claude.com/docs/en/agent-view, https://code.claude.com/docs/en/interactive-mode, https://code.claude.com/docs/en/whats-new/2026-w21].

### 1.7 Anthropic engineering posts (patterns, not just features)

**Multi-agent research system** [VERIFIED https://www.anthropic.com/engineering/multi-agent-research-system]:
- Orchestrator-worker: lead plans + spawns parallel subagents with own contexts/tools/prompts;
  subagents = "intelligent filters" (parallel gather → condensed tokens back to lead).
- Result: Opus-4 lead + Sonnet-4 workers beat single Opus-4 by **90.2%** on internal research
  eval; breadth-first queries benefit most. Token usage explains 80% of BrowseComp variance.
- Cost: agents ≈ 4× chat tokens; multi-agent ≈ **15×** chat tokens — needs high-value tasks.
  Caveat: coding has fewer truly-parallel tasks than research; models still weak at realtime
  coordination/delegation.
- Prompting rules: **teach the orchestrator to delegate** — every subtask needs objective, output
  format, tool/source guidance, clear boundaries (vague "research X" → duplication/gaps);
  **scale effort to complexity** (1 agent/3–10 calls vs 2–4 workers vs 10+ workers); start wide
  then narrow; parallel tool calls (lead spawns 3–5 workers in parallel; workers use 3+ tools
  in parallel) cut time up to 90%.
- Reliability: errors compound over long stateful runs → durable execution, resume-from-checkpoint
  (never restart-from-zero), retry logic, let the model adapt to tool failures, full production
  tracing (decision patterns, not conversation contents), rainbow deployments for live agents.
- Limitation admitted: synchronous lead-waits-for-workers creates bottlenecks (no steering,
  no worker coordination, single-straggler blocking); async adds result-coordination/state/
  error-propagation complexity.
- Eval: start with ~20 real queries immediately; single-call LLM-judge with rubric
  (factual/citation accuracy, completeness, source quality, tool efficiency) most consistent;
  humans still catch edge cases automation misses.

**Effective harnesses for long-running agents** [VERIFIED https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents]:
- Problem: compaction alone doesn't bridge context windows (half-implemented features,
  premature "done"). Fix: **initializer agent** (first session: `init.sh`, feature list,
  progress file, initial commit) + **coding agent** (every session: incremental progress, clean
  state at end).
- **Feature list as JSON** (`{category, description, steps[], passes}`), all `passes:false`
  initially; agents may only flip flags ("unacceptable to remove/edit tests"); JSON resists
  inappropriate model edits better than Markdown.
- **Clean-state invariant**: end every session committable to main (no big bugs, orderly,
  documented); use git to revert/recover.
- **Get-bearings ritual** each session: `pwd` → read progress file + feature list → `git log`
  → run `init.sh` → end-to-end smoke test as a human would (browser automation) before new work.
- Open question: single general coding agent vs specialized agents (testing/QA/cleanup).

**Harness design for long-running apps** (2026): three-agent plan/generate/evaluate split for
autonomous full-stack dev [FROM-DOCS https://www.anthropic.com/engineering/harness-design-long-running-apps,
https://www.infoq.com/news/2026/04/anthropic-three-agent-harness-ai/ (Apr 2026)].
**Managed Agents**: decouple stable interfaces ("brain") from churn-prone harness ("hands")
[FROM-DOCS https://www.anthropic.com/engineering/managed-agents].
**cwc-long-running-agents** (official repo): `/goal` = generator/evaluator loop out of the box
(fast model checks completion condition each turn); repo ships the same primitives as readable
hooks + a subagent [FROM-DOCS https://github.com/anthropics/cwc-long-running-agents].

---

## 2. OpenAI Codex (CLI + cloud) and Agents SDK

### 2.1 Codex CLI hooks

Extensibility framework running scripts or MCP tools during the loop
[VERIFIED https://developers.openai.com/codex/hooks]:
- Events: per-turn `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PreCompact`,
  `PostCompact`, `UserPromptSubmit`, `SubagentStop`, `Stop`; `Interrupt` (main thread only);
  `SessionStart`, `SubagentStart`; `SessionEnd` (main thread only)
  [VERIFIED https://developers.openai.com/codex/hooks].
- Sources: `hooks.json` or inline `[hooks]` in `config.toml`, at `~/.codex/` and
  `<repo>/.codex/`; all matching hooks from all layers run (no shadowing); plugin-bundled
  hooks supported; repo hooks load only if project layer trusted
  [VERIFIED https://developers.openai.com/codex/hooks].
- **Trust-gated**: non-managed hooks need review + hash-pinned trust via `/hooks` before
  running; `--dangerously-bypass-hook-trust` for vetted one-off automation
  [VERIFIED https://developers.openai.com/codex/hooks]. (Matches signaltty's verified finding
  on codex 0.157.1, Sept 2026 [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].)
- Timeouts: default 600s; `SessionEnd`/`Interrupt` default 1s, max 3s
  [VERIFIED https://developers.openai.com/codex/hooks].
- MCP tool hooks: call a tool on an already-connected MCP server with `${field.nested}`
  expansion; same trust/output contract; sync; errors never block
  [VERIFIED https://developers.openai.com/codex/hooks].
- Managed hooks (system/MDM/cloud/`requirements.toml`) trusted by policy; `allow_managed_hooks_only`
  drops user/project/plugin hooks [VERIFIED https://developers.openai.com/codex/hooks].
- Caveat: `prompt`/`agent` handler types are parsed but skipped; `SessionEnd` has no MCP hooks
  [VERIFIED https://developers.openai.com/codex/hooks].

### 2.2 `notify` — exactly one event

`notify = [...]` fires on **`agent-turn-complete` only**; it is not a general notification
system, reads no JSON on stdin, and cannot approve/block anything by construction
[FROM-DOCS https://backgrind.com/blog/codex-cli-notifications/ (Aug 2, 2026); corroborated by
docs/07-agents.md]. There is no `Notification` hook event in Codex — combine `notify` with
`tui.notifications` → BEL [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].

### 2.3 `codex exec` / sessions

Non-interactive `codex exec`, `codex exec resume <id|--last>`, `codex resume [id]/--last`,
`codex queue/fork/archive` [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].
Open issues (0.136–0.137 era): `exec` may not dispatch repo/user hooks even with the bypass flag
[FROM-DOCS https://github.com/openai/codex/issues/26383, https://github.com/openai/codex/issues/25875] —
hook delivery in headless runs is version-fragile; an orchestrator must not assume hooks fired.

### 2.4 Codex cloud — async task engine pattern

Tasks run in isolated cloud containers (repo checkout + setup script + secrets + network policy);
dispatch from CLI/web/Slack/GitHub/Linear; each returns summary + diff or PR; review-before-merge
is explicit ("a cloud diff is not a merge button" — inspect command log, scope, tests)
[FROM-DOCS https://help.openai.com/en/articles/20001545-using-codex-cloud,
https://eastondev.com/blog/en/posts/ai/20260708-codex-cloud-agent-workflow/ (Jul 2026)].
Delegation workflow: **plan locally (context cheap) → execute remotely (compute plentiful) →
apply diffs locally** via `codex cloud exec/list` + apply
[FROM-DOCS https://codex.danielvaughan.com/2026/05/19/codex-cli-cloud-delegation-workflows-plan-locally-execute-remotely-apply-diffs/ (May 2026)].
`@codex review` as second check before merge
[FROM-DOCS https://eastondev.com/blog/en/posts/ai/20260708-codex-cloud-agent-workflow/].

### 2.5 Agents SDK — handoffs / guardrails / orchestration patterns

Two core patterns [VERIFIED https://openai.github.io/openai-agents-js/guides/multi-agent/]:
- **Agents as tools** (`agent.asTool()`): manager keeps the conversation, calls specialists for
  bounded subtasks, owns the final answer + shared guardrails.
- **Handoffs**: triage routes to a specialist that owns the rest of the turn (context preserved,
  instructions narrowed). Combinable (triage → specialist → specialist-as-tool).
- Orchestrate via LLM (open-ended; invest in prompts, monitoring, self-critique loops,
  specialist agents, evals) vs via code (deterministic: structured outputs → branch; chained
  agents; **generator/evaluator `while` loop**; `Promise.all` parallel fan-out)
  [VERIFIED https://openai.github.io/openai-agents-js/guides/multi-agent/].

Guardrails + human review [VERIFIED https://developers.openai.com/api/docs/guides/agents/guardrails-approvals]:
- Guardrails = automatic (input/output/tool); human review = run pauses, person/policy approves.
- Approval flow is uniform: run records interruption → returns `interruptions` + resumable `state`
  → app approves/rejects → **resume the same run from `state`**; serialize `state` to resume later.
- Scope discipline: input guardrails run only for first agent, output only for final agent; put
  validation next to the side-effecting tool. Fail closed on review timeout/unavailability.
- SDK apps don't inherit Codex auto-review; add your own (open-source Codex reviewer policy at
  `codex-rs/core/src/guardian/policy.md`) [VERIFIED same URL].

---

## 3. Other agents: task model, isolation, status, results, review

### 3.1 Gemini CLI (Google)

- Headless: `-p/--prompt` bypasses chat UI, prints to stdout; piped stdin becomes context;
  `--output-format json` for script parsing (`jq -r '.response'`); non-TTY auto-headless; exit
  codes for scripting [VERIFIED https://geminicli.com/docs/cli/tutorials/automation/
  (automation tutorial); FROM-DOCS https://geminicli.com/docs/cli/headless/ (headless reference:
  JSONL event stream, output formats)].
- Hooks: scripts run **synchronously** in the loop (CLI waits for all matches); events incl.
  `SessionStart`, `BeforeTool`, …; defined in `settings.json` `hooks` object; **stdout must be
  JSON-or-silent** (same discipline as Claude/Codex Stop hooks)
  [FROM-DOCS https://geminicli.com/docs/hooks/, https://github.com/google-gemini/gemini-cli/blob/main/docs/hooks/reference.md].
- Note: unpaid/Google-One tiers migrating Gemini CLI → Antigravity CLI on Jun 18 (year per doc)
  [FROM-DOCS https://geminicli.com/docs/cli/tutorials/automation/] — CLI churn is real; isolate
  per-agent knowledge in adapters/manifests (signaltty already does [FROM-DOCS docs/07]).

### 3.2 Amp (Sourcegraph)

- **Thread = unit of work**: one conversation (prompts, replies, tool calls, changed files) per
  task; URL `ampcode.com/threads/T-…`; same thread in web/CLI/iOS/macOS; execution location
  (orb/runner/local) independent of viewing [VERIFIED https://ampcode.com/docs/threads].
- **Agent-to-agent**: an agent starts a new thread (e.g. in an orb), keeps working, the new
  thread reports back on finish; "Handoff and …" starts a fresh thread with carried context
  [FROM-DOCS https://ampcode.com/docs/orbs/agent-to-agent].
- **Orbs**: fresh isolated remote machine per thread (code, plugins, tools, parent context);
  agent keeps working with laptop closed; many parallel tasks
  [FROM-DOCS https://ampcode.com/docs/orbs].
- Specialist subagents with own context + tools; model routing per role (main/specialist/system)
  [FROM-DOCS https://ampcode.com/docs/models-and-subagents, https://ampcode.com/notes/agents-for-the-agent].
- Review UX: Changes pane (Ship/Review/Sync), intelligent file ordering for diffs, `Amp-Thread-ID`
  git trailer linking commits → thread; Activity feed with filterable URLs
  [VERIFIED https://ampcode.com/docs/threads].

### 3.3 Cursor (background agents + worktrees)

- Agents Window runs background agents; **worktree per task**: separate checkout+branch per agent,
  main checkout untouched; review in window → commit/PR from worktree or bring back to main
  [VERIFIED https://cursor.com/docs/configuration/worktrees].
- Setup automation: `.cursor/worktrees.json` (`setup-worktree-unix|-windows|generic`: command
  array or script path) runs on every worktree creation (Agents Window, IDE, CLI)
  [VERIFIED https://cursor.com/docs/configuration/worktrees].
- IDE skills: `/worktree` (rest of chat in new checkout), `/apply-worktree` (bring changes to
  main), `/delete-worktree`, `/best-of-n` (same task × N models, one worktree each, compare,
  no auto-merge — human picks winner) [VERIFIED https://cursor.com/docs/configuration/worktrees].
- Lifecycle ops most orchestrators forget: **cleanup** (`cursor.worktreeMaxCount` default 25/
  machine, `cursor.worktreeCleanupIntervalHours`, catch-up after restart, burst debounce) and
  **discovery** (mtime-checkpoint re-scan so externally-created worktrees aren't skipped)
  [VERIFIED https://cursor.com/docs/configuration/worktrees].
- CLI: `cursor-agent --resume/--continue`, `--print --output-format json|stream-json`,
  `--model`, `--mode plan|ask` [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md].

### 3.4 GitHub Copilot coding agent

- Async agents (Copilot, Claude, Codex, custom) dispatched from issues/PRs/agents panel; model
  picker per task (fast model for tests, strong for refactors, or auto); plan → code → PR
  [FROM-DOCS https://github.blog/ai-and-ml/github-copilot/whats-new-with-github-copilot-coding-agent/,
  https://github.com/features/copilot/agents].
- Review discipline: **read the session log first, then the diff**; Actions workflows do NOT
  auto-run on agent pushes — maintainer must "Approve and run workflows"
  [FROM-DOCS https://prlens.dev/guides/how-to-review-copilot-coding-agent-pull-requests].
- Code review runs on agentic tool-calling architecture (GA Mar 5, 2026), auto-resolves comments,
  shell-validates code, ensemble agents (Sep 11, 2026)
  [FROM-DOCS https://github.blog/changelog/2026-03-05-copilot-code-review-now-runs-on-an-agentic-architecture/,
  https://github.blog/changelog/2026-09-11-auto-resolution-and-analysis-updates-in-copilot-code-review/].

### 3.5 Jules (Google)

Async agent: assign via prompt or `jules` GitHub label → clones repo into secure Cloud VM →
plans (Gemini 3 Pro) → multi-file edits → diff browse/approve → PR; audio changelogs; free tier
15 tasks/day; public beta May 20 2025, GA Aug 6 2025
[FROM-DOCS https://jules.google/, https://blog.google/innovation-and-ai/models-and-research/google-labs/jules/,
https://techcrunch.com/2025/08/06/googles-ai-coding-agent-jules-is-now-out-of-beta/].

### 3.6 Devin / OpenHands

- OpenHands 1.0 (Sep 8, 2026): production-grade Docker sandbox, self-hosted/auditable,
  SWE-bench Verified 68% (Qwen3-Coder-480B) / 72% (Sonnet 4.5 + extended thinking); PR-ready
  diffs + in-sandbox tests as the core loop
  [FROM-DOCS https://the-agent-report.com/2026/09/openhands-1-0-coding-agent-sandbox/].
- Works best with clear requirements, well-defined scope, existing patterns, good tests,
  isolated changes; explicit agent task-state control (pause/resume/cancel) in UI
  [FROM-DOCS https://docs.openhands.dev/openhands/usage/essential-guidelines/when-to-use-openhands,
  https://github.com/OpenDevin/OpenDevin/pull/1094].
- Devin = closed full-service counterpart (same task→sandbox→PR shape, black-box infra)
  [FROM-DOCS https://codeables.dev/article/openhands-vs-devin-which-one-is-better-at-producing-pr-ready-diffs].

### 3.7 OpenCode

`opencode serve` = headless HTTP + OpenAPI on 127.0.0.1:4096; `opencode run [--attach URL]
[--session] [prompt]` headless execution; session create/get API; plugin hooks (25+ events:
`session.created/updated/idle/status`, `command.execute.before`, …)
[FROM-DOCS https://opencode.ai/docs/server/, https://opencode.ai/docs/cli/,
/home/simone/code/signaltty/docs/07-agents.md]. Closest to "agent as API server" among CLIs.

---

## 4. Protocols: what a local terminal orchestrator should expose/consume

### 4.1 ACP (Agent Client Protocol) — most relevant shape

JSON-RPC 2.0 client↔agent protocol; agents = programs that autonomously modify code
[VERIFIED https://agentclientprotocol.com/protocol/v1/overview + /protocol/v1/schema].
- Agent methods: `initialize` (version + capability negotiation, auth methods),
  `authenticate`, `session/new` (`new_session`), `session/prompt`, `session/load` (resume +
  replay history), `session/list` (filter by cwd, cursor pagination), `session/close`,
  `session/delete`, `session/cancel` (notification: stop LLM calls, abort tools, flush updates,
  reply `StopReason::Cancelled`) [VERIFIED https://agentclientprotocol.com/protocol/v1/schema].
- Client methods/notifications: `session/update` (streaming progress incl. tool calls),
  `session/request_permission`, `fs/read_text_file` (+write), `elicitation/create`
  [FROM-DOCS https://agentclientprotocol.com/protocol/v1/schema (method index)].
- Semantics worth stealing: capability-gated methods (`sessionCapabilities.*`), `_meta`
  extensibility reservation, auth-required flow before session creation, cancel-then-free
  ordering in close [VERIFIED https://agentclientprotocol.com/protocol/v1/schema].
- Status: v2 Draft published Jul 20, 2026 (breaking changes for hard-to-express uses); Registry
  RFD completed (standard discovery/configure metadata)
  [FROM-DOCS https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/announcements/acp-v2-draft.mdx,
  https://agentclientprotocol.com/updates].
- Fit for signaltty: ACP is the **client↔agent** axis (IDE drives agent). signaltty is a
  **multiplexer/orchestrator** axis (many panes, PTYs, git). Consuming ACP (drive ACP-speaking
  agents via `session/prompt` + `session/update` instead of keystrokes) is the high-value
  direction; exposing signaltty *as* an ACP agent is a stretch goal [INFERRED].

### 4.2 A2A (Agent2Agent) — best task-lifecycle reference

Discussed: stateless `Message` vs stateful `Task`
[VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/]:
- Respond `Message` for trivial/negotiation exchanges; `Task` for substantial trackable work.
  Task states: working → `input-required`/`auth-required` (interrupted) or `completed`/
  `canceled`/`rejected`/`failed` (terminal) [VERIFIED same URL].
- `contextId` groups tasks+messages into a collaboration; `referenceTaskIds` links refinements;
  follow-ups = **new task, same context**; terminal tasks are **immutable** (never restart —
  clean input→output mapping, traceability) [VERIFIED same URL].
- Artifacts: agent emits named artifacts; refinements reuse `artifact-name` with new
  `artifactId`; **client tracks version history** and shows latest acceptable; agent infers or
  asks (`input-required`) on ambiguity [VERIFIED same URL].
- Parallel follow-ups: distinct parallel tasks per follow-up message in one context; new
  dependent tasks as soon as prerequisites complete [VERIFIED same URL].
- Fit: A2A's Task/artifact/contextId/immutability model maps 1:1 onto orchestrator needs
  (task entity, result handoff, refinement chains). Adopt the semantics; JSON-RPC wire compat
  is optional [INFERRED].

### 4.3 MCP (Model Context Protocol) — expose orchestration here, yes

Client-host-server, JSON-RPC, tools/resources/prompts/roots
[FROM-DOCS https://modelcontextprotocol.io/specification/2026-07-28/architecture (spec 2026-07-28)].
- **Elicitation**: server requests structured info from user mid-run (form mode w/ JSON-schema
  validation, URL mode); client mediates
  [FROM-DOCS https://modelcontextprotocol.io/specification/2026-07-28/client/elicitation].
- **Sampling**: server asks client for LLM completions (server-side agentic loops on client
  tokens, under user supervision); SEP "sampling-with-tools" adds `tools`/`toolChoice` to
  `sampling/createMessage`, deprecating `includeContext`
  [FROM-DOCS https://github.com/modelcontextprotocol/modelcontextprotocol/blob/02dd8f61/seps/1577--sampling-with-tools.md].
- Fit: **yes, expose orchestration as an MCP server in addition to the CLI.** Every major CLI
  (Claude `--mcp-config`, Codex MCP tool hooks, Gemini, OpenCode `serve`) already consumes MCP
  tools; MCP gives the in-pane orchestrator typed tools (spawn/wait/read/result) with elicitation
  for approvals and sampling for summarize-then-return — no PTY keystroke tricks, no new client
  to install. CLI stays for scripts/humans; MCP is for agents [INFERRED, grounded in the MCP +
  CLI sources above].

---

## 5. Cross-cutting best practices (distilled)

**P1. Orchestrator-worker, with the right decomposition.** Lead decomposes at runtime, workers
run parallel in isolated contexts, lead synthesizes. Mature pattern for "right decomposition
unknown in advance" (audit a repo, research a topic); two variants: delegation (own the answer)
vs fan-out synthesis. Overhead is real — don't parallelize sequential/same-file work
[FROM-DOCS https://agentpatternscatalog/patterns (orchestrator-workers: mature),
https://agentpatterns.ai/patterns/multi-agent/orchestrator-worker/,
https://code.claude.com/docs/en/agent-teams (comparison table)].

**P2. Task contract.** Every delegated unit carries: objective, output format, tool/source
guidance, explicit boundaries. Anthropic's failure mode without it: duplicated searches, gaps,
50 subagents for simple queries [VERIFIED https://www.anthropic.com/engineering/multi-agent-research-system].
Scale worker count + tool budget to complexity explicitly (1/3–10 calls … 10+ workers)
[same URL]. JSON beats Markdown for machine-edited state (feature list, progress)
[VERIFIED https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents].

**P3. Context isolation.** Own window per worker (Claude subagents, Amp threads, A2A contextId);
one thread/task per concern; compaction loses detail → re-inject via `SessionStart:compact`
[VERIFIED https://code.claude.com/docs/en/sub-agents, https://code.claude.com/docs/en/hooks;
VERIFIED https://ampcode.com/docs/threads].

**P4. Structured result handoff.** Results are artifacts with stable names + new IDs per revision
(A2A); schema-validated output (`--json-schema` → `structured_output`, Gemini `--output-format
json`, MCP elicitation form mode); thread/commit linkage (`Amp-Thread-ID` trailer)
[VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/,
https://code.claude.com/docs/en/headless; VERIFIED https://ampcode.com/docs/threads].

**P5. Hooks ≫ screen scraping for readiness/completion.** All CLIs now emit lifecycle hooks
(Claude `Stop`/`SubagentStop`/`Notification:*`, Codex `Stop`/`SubagentStop`+`notify`,
Gemini sync hooks, OpenCode `session.status`, Cursor `hooks.json sessionStart`). signaltty's
priority stack (native → hooks → OSC → /proc → title → heuristics-last) matches the industry
direction [FROM-DOCS /home/simone/code/signaltty/docs/07-agents.md + hook sources above].
Headless hook delivery is version-fragile (Codex exec issues) → treat hook absence as a signal,
keep wait-baseline + timeout fallbacks [FROM-DOCS codex issues §2.3; FROM-DOCS research-codebase.md §3].

**P6. Idempotency + crash recovery.** Resume-from-checkpoint, never restart-from-zero (Anthropic);
`init.sh` + progress file + git log as the re-orientation ritual; clean-state invariant per
session; resumable run `state` (OpenAI interruptions); immutable terminal tasks, refinements as
new tasks (A2A); rainbow deployments for live agents; wait baselines with `IDENTITY_CHANGED`
(signaltty already has) [VERIFIED anthropic + a2a URLs above; VERIFIED openai guardrails URL;
FROM-DOCS research-codebase.md §3].

**P7. Concurrency limits + cost budgets.** Multi-agent ≈ 15× chat tokens (Anthropic); per-run
cost in-band (`total_cost_usd`); model routing (strong lead, cheap explorers); explicit worker
caps; rate-limit/stop-failure handling (`StopFailure`, `quota_auto_resume_*` matchers exist
because runs die on quotas) [VERIFIED anthropic + headless + hooks URLs above].

**P8. Conflict avoidance = worktree-per-task + ownership + serialized landing.**
Universal: one worktree+branch per task (Claude `--worktree` + 4 enforcement checks, Cursor,
coding-agent guides); clean base from `origin/HEAD`, never a dirty tree; `.worktreeinclude`/
`worktrees.json` for setup reproducibility; ownership beyond git (task↔worktree↔lock mapping,
intent-carrying claims, partition by operation not just folder); merge-conflict prediction +
test-gated serialized landing queue; `/best-of-n` keeps candidates isolated until a human picks
[VERIFIED https://code.claude.com/docs/en/worktrees, https://cursor.com/docs/configuration/worktrees;
FROM-DOCS https://codingagentguide.com/posts/worktree-isolation-rules-for-parallel-coding-agents/,
https://brandonwie.dev/posts/parallel-agents-no-collisions, https://github.com/alwh1te/agent-semaphore,
https://aiarch.dev/workflows/parallel-agent-writes].

**P9. Verification gates before merge.** Generator/evaluator loop (`/goal`, OpenAI `while`+eval,
Codex `@codex review`, Copilot agentic review); session-log-first review + no-auto-CI-on-agent-push
(Copilot); diff-not-merge-button discipline (Codex cloud); human picks winner for parallel
candidates (Cursor best-of-n); end-to-end self-test as a human would (Anthropic harness)
[VERIFIED/FROM-DOCS urls in §1.7, §2.4–2.5, §3.3–3.4].

**P10. Human-in-the-loop approval.** Risk-tiered (pause only where judgment changes outcome; AWS);
uniform interruption→approve/reject→resume-same-run (OpenAI); tamper-evident audit of decisions
(15-factor gates); fail closed on timeout; signaltty's native permission bridge + `decision.*`
events already fit [VERIFIED openai guardrails URL; FROM-DOCS https://docs.aws.amazon.com/wellarchitected/latest/agentic-ai-lens/agentsec04-bp02.html,
https://github.com/pliuz/15-factor-approval-gates; FROM-DOCS docs/08-ipc.md `decision.*`].

**P11. Observability = event log per task.** Streaming status + artifact updates (A2A events, ACP
`session/update`, `stream-json` lines with `parent_tool_use_id`); full tracing of decision
patterns (Anthropic); signaltty's `audit.jsonl` + `subscribe{from_seq}` replay is the same idea
[VERIFIED a2a/headless URLs; FROM-DOCS docs/08-ipc.md].

---

## 6. Must-have for signaltty 018 (ranked)

1. **Task entity with A2A lifecycle + immutable terminal state.** `pending/working/input-required/
   completed/canceled/rejected/failed`; follow-ups are new tasks in the same context; artifacts
   named + versioned. Why: every protocol and cloud agent converged here; needed for merge/discard
   review. Source: [VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/].
2. **Worktree-per-worker with base-ref discipline + setup script.** New branch from recorded
   clean base (origin/HEAD SHA captured at spawn — signaltty diffs are worktree-vs-HEAD only
   today, so the orchestrator must record the base itself); `.worktreeinclude`-style env carry;
   discovery + bounded cleanup (Cursor's 25-machine-cap pattern). Why: the universal conflict
   avoidance; audit flags base-SHA + cleanup gaps. Sources: [VERIFIED https://code.claude.com/docs/en/worktrees,
   https://cursor.com/docs/configuration/worktrees; FROM-DOCS research-codebase.md §6/§9].
3. **Task contract schema (objective, constraints, acceptance criteria, output format, tool/file
   budget).** Enforced at spawn; passed verbatim to the worker prompt. Why: Anthropic's #1
   delegation failure was vague briefs → duplication/gaps. Source: [VERIFIED https://www.anthropic.com/engineering/multi-agent-research-system].
4. **Hook-based readiness/completion (Stop/SubagentStop/idle/turn-complete) + wait-baseline
   discipline; scraping only as last resort.** Extend `wait`/`pane.get` baseline pattern to tasks;
   treat missing hooks (headless fragility) as explicit degraded mode with timeouts. Why: all
   vendors' direction; signaltty stack already built for it. Sources: [VERIFIED claude hooks +
   codex hooks URLs; FROM-DOCS docs/07-agents.md, research-codebase.md §3].
5. **Reliable prompt delivery: bracketed-paste wrap + `\r` submit + readiness gate.** `pane.input`
   raw bytes don't submit in TUIs today; delivery must wait for Idle/Done and confirm echo.
   Why: audit §2 proves `\n`-only input fails; multiline needs `ESC[200~…ESC[201~` + `\r`.
   Sources: [FROM-DOCS research-codebase.md §2; INFERRED for confirm-echo].
6. **Readable worker output: scrollback-backed structured read.** vt100 scrollback is 0 today;
   tail mode garbles TUIs. Need scrollback + grid extraction (and diff-between-reads) so the
   orchestrator can read results without attaching a GUI. Why: audit §1 blocks output reading
   entirely. Source: [FROM-DOCS research-codebase.md §1].
7. **Parentage + labels on panes/tasks (caller id, task id, spawner chain).** `$SIGNALTTY_PANE`
   is clobbered on spawn today; ownership (who spawned whom, which task owns which worktree)
   is required for collision-free fan-out. Why: P8 ownership; Cursor/Amp thread linkage.
   Sources: [FROM-DOCS research-codebase.md §5; VERIFIED https://ampcode.com/docs/threads].
8. **Structured result handoff (JSON artifact + schema).** Worker returns machine-readable result
   (summary, files changed, test evidence, base→head SHAs); orchestrator validates. Why: A2A
   artifacts; `--json-schema`; Copilot session-log-first review. Sources: [VERIFIED a2a +
   headless URLs; FROM-DOCS prlens review guide].
9. **Review gate before merge: diff + session log + verification evidence, human decision.**
   No auto-merge; test-gated serialized landing; `/best-of-n`-style pick-winner for parallel
   candidates. Why: Codex/Copilot/Cursor consensus. Sources: [FROM-DOCS §2.4, §3.3, §3.4].
10. **Expose orchestration as an MCP server (spawn/wait/read/result/approve) alongside the CLI.**
    Why: in-pane orchestrators are MCP-native in every CLI; elicitation covers approvals,
    sampling covers summarize-then-return; CLI remains for humans/scripts. Sources:
    [FROM-DOCS MCP spec 2026-07-28 URLs; INFERRED fit].
11. **Concurrency caps + cost/rate-limit budgets per task tree.** Max parallel workers, tool-call
    budgets, quota-failure states with backoff. Why: 15× token multiplier; quota deaths are
    common enough to have dedicated hook matchers. Sources: [VERIFIED anthropic research +
    claude hooks/headless URLs].
12. **Crash recovery: persisted task state + resume-offer (never silent restart).** Tasks in
    snapshot (with `#[serde(default)]`), audit-logged transitions, resumable waits; worker pane
    death → task `failed` with evidence, not limbo. Why: Anthropic checkpoints; OpenAI resumable
    state; audit §7 migration invariant. Sources: [VERIFIED anthropic + openai URLs;
    FROM-DOCS research-codebase.md §7].
13. **Per-task event log + task-scoped subscribe.** All task transitions, worker lifecycle, diffs,
    decisions replayable from a cursor. Why: P11; signaltty `subscribe{from_seq}` generalizes
    naturally. Sources: [VERIFIED a2a URL; FROM-DOCS docs/08-ipc.md].
14. **Generator/evaluator self-check before human review.** Fast-model or rule-based acceptance
    pass on worker output (acceptance criteria from the contract) to filter obvious failures.
    Why: `/goal` loop; OpenAI evaluator pattern; Anthropic LLM-judge. Sources: [FROM-DOCS
    cwc-long-running-agents; VERIFIED openai multi-agent + anthropic research URLs].

## 7. Explicitly avoid

- **Screen scraping as the primary completion signal.** All vendors moved to hooks/events;
  scraping is last-resort by consensus (incl. signaltty's own stack). Don't build 018's core
  loop on spinner/prompt regex [VERIFIED hook URLs; FROM-DOCS docs/07-agents.md].
- **Shared-checkout parallel workers.** Partition-by-folder alone fails (bulk sweeps claim by
  pattern; lost writes raise no error); always isolate by worktree, and add ownership claims
  when sharing is unavoidable [FROM-DOCS aiarch + agent-semaphore + brandonwie URLs].
- **Dirty-tree bases.** Branching workers from uncommitted state hides the task diff and breaks
  review; record a clean base SHA per task [VERIFIED claude worktrees URL; FROM-DOCS audit §6].
- **Auto-merge / auto-CI on agent output.** No vendor does it: diffs need human (or gated)
  review; Copilot blocks Actions until a maintainer approves [FROM-DOCS prlens + codex-cloud URLs].
- **Restart-from-zero on failure.** Expensive + frustrating; resume from checkpoint/state, keep
  evidence, mark terminal states immutable [VERIFIED anthropic + a2a URLs].
- **Unbounded fan-out.** No worker caps / tool budgets → 15× token bills and quota deaths;
  scale effort to complexity explicitly [VERIFIED anthropic research URL].
- **Rubber-stamp HITL (approving everything or nothing).** Risk-tier approvals with full context
  (diff + log + proposed action) and fail-closed timeouts; record decisions in the audit log
  [VERIFIED openai guardrails URL; FROM-DOCS aws + 15-factor URLs].
- **Trusting headless hook delivery blindly.** `codex exec` hook gaps and trust-gating mean hooks
  may never fire; degrade explicitly (timeout + state poll + flag) [FROM-DOCS codex issues §2.3].
- **Exposing signaltty as a full ACP agent in v1.** Consume agent protocols where they exist
  (OpenCode `serve`, ACP `session/prompt` later); the multiplexer axis (PTYs, panes, git) is the
  product — don't dilute 018 into a protocol-compat project [INFERRED].
- **Plain-text stdout from hook shims.** Claude/Codex/Gemini Stop hooks fail or misbehave on
  non-JSON stdout (signaltty already prints to stderr); keep shims silent-or-JSON [VERIFIED claude
  hooks + codex hooks URLs; FROM-DOCS docs/07-agents.md].
