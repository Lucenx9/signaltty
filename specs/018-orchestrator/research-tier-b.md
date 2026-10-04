# 018 orchestrator — Tier-B research (Superset, Oh My Pi, Traycer, Hermes)

Research date: 2026-10-04. Public repos shallow-cloned under `/tmp/tierb` (+`/tmp/hc`) and read there.
Nothing in the signaltty tree was changed. Companion docs `research-external.md`
(t3code/cmux/herdr/workmux/Conductor/vibe-kanban/claude-squad) and `research-practices.md`
(Claude/Codex/ACP/A2A/MCP) are not re-covered; this file only adds what Tier B teaches.

Claim labels: **VERIFIED-IN-SOURCE** (path in the named clone), **FROM-DOCS** (official
docs URL), **INFERRED** (synthesis, no single source).

## Clones

| Repo | SHA | Date | Note |
| --- | --- | --- | --- |
| superset-sh/superset | `c711aa02da3f` | 2026-10-03 | Electron desktop, ELv2 source-available, not OSI-open |
| traycerai/traycer | `745704377226` | 2026-10-04 | MIT, desktop + protocol packages |
| NousResearch/hermes-agent | `64ad33e32dad` | 2026-10-03 | MIT, Python agent framework |
| can1357/oh-my-pi (omp core) | `6d8552d7f9df` | 2026-10-04 | omp.sh backend, `@oh-my-pi/pi-coding-agent` |
| forcewake/hermes-conductor | (shallow, Oct-04) | — | third-party Hermes-ecosystem patterns, NOT Nous Research |

"Oh My Pi (omp.sh)" = omp, the Pi fork (single Bun process, `~/.omp/agent/`);
the `shujunqiao/oh-my-pi-ai` swarm-extension YAML DAG is a different community project and is
only noted in passing. "Hermes agent (Nous Research)" = `hermes-agent`; `hermes-conductor`
is covered separately as ecosystem evidence (§5) because its incident-grounded rules are
exactly the failure modes 018 must survive.

---

## 1. Superset (superset.sh) — the agent-agnostic workspace + coordinator skill

### Architecture & stack

Electron desktop + local host-service + CLI + TS SDK + MCP server (Streamable HTTP) + A2A
agent card + mobile app; macOS + Linux (experimental AppImage), no Windows
(**FROM-DOCS** https://superset.sh/llms.txt). Not a coding agent itself: "the workspace and
orchestration layer the agents run in" (**FROM-DOCS** same). Unit of work = **workspace**
(one branch + one checkout + running environment); unit of execution = **agent session**
(one agent conversation in one terminal) + **terminal** (PTY the CLI addresses by id)
(**VERIFIED-IN-SOURCE** `superset-sh_superset/apps/docs/content/docs/agent-sessions.mdx`,
`packages/cli/src/commands/{agents,terminals,workspaces}`). Terminals survive Electron
restarts via a dedicated pty-daemon process with snapshot/handoff
(**VERIFIED-IN-SOURCE** `superset-sh_superset/packages/pty-daemon/src/main.ts`;
**FROM-DOCS** https://superset.sh/blog/terminal-daemon-deep-dive).
Relevant prior art already in 018's orbit: Superset publishes `superset-vs-{herdr,cmux,
t3-code,vibe-kanban,claude-squad,conductor}` comparisons (**FROM-DOCS** llms.txt index).

### Task / work-unit model and states

Two deliberately separate things (**VERIFIED-IN-SOURCE** `plugins/superset/skills/orchestrate/SKILL.md`):
**organization tasks** (Linear/GitHub-synced issue tracker rows with list/board views) are
NOT orchestration DAG nodes; the **coordinator table** (Task, Dependencies, Workspace, Host,
Terminal, Status, Result) lives in the coordinator agent's own context. Coordinator statuses:
`pending, ready, running, completed, blocked, failed` (same file). Promotion rule: pending →
ready only when every dependency is completed; redispatch a failure only after changing
prompt/inputs/worker, stop after repeats. **INFERRED**: no durable server-side task state
machine for orchestration — durability comes from workspaces/sessions, not from a task row.
018 already decided the opposite (durable Task entity); Superset is evidence for why 018's
choice is stronger for crash survival.

### Agent spawn + isolation + base ref + cleanup

`superset workspaces create` (CLI) with `--checkout worktree` (default) vs `--checkout local`
(shared checkout), `--branch` (namespaced under project branch prefix unless
`--skipBranchPrefix`), `--baseBranch` to fork from when the branch is new (defaults to
project default), plus `--pr` (verified PR head checkout), `--task` (link issue; provider
branch name used verbatim), `--session` (project-less scratch), cloud vs `--local`/`--host`
placement (**VERIFIED-IN-SOURCE** `superset-sh_superset/packages/cli/src/commands/workspaces/create/command.ts`).
`superset agents create --workspace --agent <preset> --prompt` launches a worker; result is
`{kind, sessionId, label}`, `kind: terminal` required by the skill (same SKILL.md).
Per-project lifecycle scripts in `.superset/config.json`: `setup` (runs at workspace
creation), `teardown` (at deletion), `run` (dev server on demand); opt-in
"wait for setup before starting agents" chains agent start after setup in the same terminal
(**FROM-DOCS** `setup-teardown-scripts.mdx`). Cleanup: closing a terminal ends the session;
deleting a workspace is a separate destructive action (SKILL.md). No auto-merge anywhere:
"Superset does not merge worker branches automatically" (same SKILL.md).

### Readiness / turn-completion / blocked detection

Lifecycle hooks + command wrappers installed into each agent's config (`~/.superset/bin`,
`superset-hooks` files) report start/finish/waiting-for-input; the hooks no-op outside
Superset terminals (env-var gate) and are removable via a per-agent toggle
(**FROM-DOCS** `agent-status.mdx`). Hook shim
(**VERIFIED-IN-SOURCE** `superset-sh_superset/packages/agent-setup/templates/notify-hook.template.sh`,
318 lines) normalizes across harnesses: Claude/Mastra/Droid/Kimi/Grok via stdin,
Codex JSON via argv; maps `agent-turn-complete|task_complete → Stop`, `task_started → Start`,
`exec_approval_request|apply_patch_approval_request|request_user_input → PermissionRequest`;
Grok lowercase `notification` counts only for `permission_prompt|elicitation_dialog`;
**never defaults to Stop on parse failure** ("silent drop is safer than a false completion
notification"). Subagent events (`agent_id` set) are forwarded to a host roster only and
must NOT drive terminal-level status or session-id binding — same rule as cmux/t3code.
Identity-disagreement guard: wrapper-exported `SUPERSET_AGENT_ID` (first-wins) vs config
`SUPERSET_HOOK_HARNESS`; on mismatch the event is dropped (covers cursor-agent replaying
Claude config, nested `codex exec` inside a Claude terminal). Endpoint resolution is
restart-proof: frozen env URL first, then live per-org manifest endpoints. Support varies by
agent: Claude reports finished+waiting; some agents emit no waiting signal → completion-only
notifications (agent-status.mdx). Stale "running" after force-quit is user-cleared via
"Clear Status" — **INFERRED**: no pid/command/boot-id crash-vs-recycle evidence like
workmux `AgentState`; a gap 018 already closes with `worker_pid/worker_cmd`.

### Prompt delivery and follow-ups

`superset terminals send --workspace --terminal --text [--noSubmit]` stages text with or
without Enter (**VERIFIED-IN-SOURCE** `superset-sh_superset/packages/cli/src/commands/terminals/send/command.ts`).
No readiness gate, no draft/dialog guard, no activity-gate/stall detection at the CLI layer —
the coordinator skill compensates procedurally ("read all workers each pass", "prefer short
passes over one long blocking loop"). Delivery target for review comments: a running agent
terminal in the PR-linked workspace, else create a PR-checkout workspace with the comment as
first prompt (**FROM-DOCS** `pull-requests.mdx`). **INFERRED**: strictly weaker than
herdr-style gated submit; confirms 018's FR-003–FR-005 as differentiating, not table stakes.

### Output reading

`superset terminals read --terminal --max-lines` returns a current-screen snapshot as text
(**VERIFIED-IN-SOURCE** `.../terminals/read/command.ts`); the skill warns: "Terminal
discovery does not identify semantic recipients or agent completion state… do not infer
completion from presence, absence, `attached`, or terminal title alone". `superset agents
read` returns the agent's **saved conversation** (Claude/Codex only; fails otherwise, no
terminal-output fallback) — transcript-over-scrollback, same direction as Traycer
(**FROM-DOCS** `agent-sessions.mdx`). No incremental cursor anywhere (same gap as
herdr `revision: 0`).

### Structured results and parent/child

Worker completion is a **prompt-convention envelope**, not a durable event: the worker's
final response must end with `SUPERSET_WORKER_DONE` (task/summary/files/checks/handoff) or
`SUPERSET_WORKER_BLOCKED` (task/reason/needs); "markers are a prompt convention visible in
terminal snapshots, not durable Superset events. Treat malformed or missing envelopes as
unstructured output" (SKILL.md). Parent/child lineage: sessions support resume
(`--resume-session`, auto-resume on unexpected death with "Resuming…" pill, deliberate
closes stay down), fork (agent-native fork commands per agent table: Claude
`--resume --fork-session`, Codex `codex fork`, OpenCode/Pi/Grok/Droid variants), and
**continue-with-another-agent** handoff (last 36k chars as read-only history + `git status`
grounding; explicit credential-leak warning) (**FROM-DOCS** `agent-sessions.mdx`).
**INFERRED**: envelope-without-event is the exact weakness 018's `task.report` + `task.result`
event + future inbox fixes; Superset is the worked example of why the event must exist.

### Planning / spec-first

No plan-first flow; the skill demands bounded prompts (objective, scope/files, acceptance +
verification commands, dependencies, anti-scope warning) per worker — a lighter-weight
cousin of 018's contract (FR-010). Verification is coordinator-owned: "Mark completed only
after reading DONE envelope **and checking its evidence**… Independently verify risky or
overlapping changes" (SKILL.md).

### Review / merge / PR / CI

PR view (list/detail with checks summary, diff viewer with file tree + unified/side-by-side,
comment thread + reply composer); merge actions = squash / merge-commit / rebase; "comment
straight to an agent" from selected diff lines (**FROM-DOCS** `pull-requests.mdx`).
Review-then-PR-per-worker is the stated integration path ("You review the results the same
way as always: one branch and PR per worker", orchestration doc). No CI gating or
test-evidence automation server-side.

### Concurrency & cost limits

Remote-host dispatch + `hosts wake`; scheduled recurring runs via **Automations**
(cron-like prompt execution in fresh/existing workspace) (**FROM-DOCS** llms.txt,
orchestration doc). No worker cap, no per-task cost accounting found in the CLI/skill
surface. (`superset agents create` also takes `--effort`/`--model` per-agent routing.)

### The UX that makes it rank high

One dashboard for heterogeneous agents (Claude/Codex/OpenCode/Gemini/Copilot/Cursor) with
activity strip per workspace, agent chips, dock badge for needs-attention, mobile app
("leave your desk, keep building"), run-summary **Pages** (persistent shareable report
per multi-workspace run, commentable per row), port management, setup/teardown/run scripts.
Rank comes from **breadth + stability of the workflow shell**, not orchestration depth.

### Distinctive features worth stealing

1. **Handoff with transcript grounding + credential warning** (36k-char read-only history,
`git status` as source of truth, explicit secret-leak callout) — 019 board/follow-up fodder
for a `task.handoff` verb.
2. **Resume-pill + fork-matrix per agent** (native fork commands table, resume-args per
agent) — adopt as a resume-strategy table for 018 workers instead of one-size-fits-all.
3. **`agents read` (saved transcript, no scrollback fallback)** as the output-reading
hierarchy: saved transcript > rendered grid > raw bytes — matches 018's hooks-first stack.
4. **Never-default-to-Stop + harness-disagreement-drop** in the hook shim — two lines for
018's hook-event receiver spec.
5. **Run-summary Pages** — the "one link per parallel run" review artifact; 019 candidate.

---

## 2. Oh My Pi / omp (omp.sh; core: `can1357/oh-my-pi`)

### Architecture & stack

Terminal-first coding agent, fork of Mario Zechner's Pi; single Bun process; flat tool
surface; any model provider; sessions as local JSONL (`~/.omp/agent/sessions/`, resume/fork/
branch/share); settings/creds/plugins/caches under `~/.omp/agent/`
(**FROM-DOCS** https://omp.sh/docs). Orchestration is **in-process**: `task` tool spawns
child sessions in the same process; children coordinate over a process-global **IRC bus**;
Agent Hub (Alt+A) is the roster/inspector TUI (**FROM-DOCS** https://omp.sh/docs/subagents;
**VERIFIED-IN-SOURCE** `oh-my-pi/packages/coding-agent/src/irc/bus.ts`,
`packages/coding-agent/src/irc/messaging.ts`). Related Pi-family worktree pattern:
community `freespace8/omp-side-agents` runs each child in its own tmux window + git
worktree (**FROM-DOCS** search result) — i.e. the ecosystem already reaches for exactly
018's shape when it needs real isolation. (`shujunqiao/oh-my-pi-ai` swarm-extension: YAML
DAG → full-subagent pipeline, shared-filesystem communication — community alternative,
not evaluated in depth.)

### Task / work-unit model and states

`task` tool takes parallel entries each with self-contained `assignment` (+ shared batch
`context`); per-agent specialist routing (`scout/designer/reviewer/security-reviewer/
librarian/task/sonic`), optional JSON-schema `outputSchema` per task with validator +
exactly-one bounded retry carrying validation errors verbatim
(**VERIFIED-IN-SOURCE** `oh-my-pi/packages/coding-agent/src/task/types.ts`,
`packages/coding-agent/src/task/executor.ts` `outputSchema`, `resolveFallbackCompletion`).
Worker lifecycle in Agent Hub: `running` (active turn) / `idle` (turn done, session live
for follow-up) / `parked` (saved to disk, revivable) / `aborted` (terminal, transcript
kept); finished workers stay associated; resume restores lineage
(**FROM-DOCS** subagents page). Keep-alive work pools with queued/running/completed/failed/
cancelled item states exist for aggregate jobs (**VERIFIED-IN-SOURCE**
`oh-my-pi/packages/coding-agent/src/task/workpool.ts`). Per-spawn prompt carries context,
worktree path, and IRC roster (**VERIFIED-IN-SOURCE** executor.ts ~l.3590).

### Agent spawn + isolation + base ref + cleanup

Default: subagents share the parent checkout (fast, mutually visible) with file-disjoint
ownership as the recommended guard. Opt-in isolation (`Tasks → Isolation → Auto`): per-worker
filesystem clone/overlay (platform-selected backend) rooted at cwd, normally under
`~/.omp/wt` (`worktree.base` relocates); baseline captured before the run; integration via
**patch mode** (default: `${agentId}.patch` + nested-repo patches applied to parent) or
**branch mode** (commit onto `omp/task/${agentId}`); failed merges keep the branch for
manual resolution; un-capturable workspaces are retained under unique `.retained-*` siblings
with the path named in the error; one-shot/failed startups clean up in `finally`; kept-alive
runs capture on release with fingerprint dedup against the already-handed-off delta
(**VERIFIED-IN-SOURCE** `oh-my-pi/packages/coding-agent/src/task/isolation-runner.ts`,
`isolation-ownership.ts`; **FROM-DOCS** subagents "Shared checkout or isolated workspace").
Isolation is explicitly NOT a security sandbox (commands/network/side effects unaffected).
Limitation that validates 018's model: isolated workers are never revivable post-teardown;
transcript + patch metadata stay inspectable (docs). No branch-from-base-ref discipline
found — isolation clones the live checkout, i.e. the dirty-base hazard 018 refuses.

### Readiness / turn-completion / blocked detection

Turns are first-class (provider loop), not hooks: results "delivered as each agent yields";
soft request budget (default 200, wrap-up ask then force partial at 1.5×) + optional hard
`maxRuntimeMs` (**FROM-DOCS** subagents settings table). Stuck/unsafe/out-of-scope workers
are killed (`x` in Hub: immediate kill + release). No permission-blocked state machine
surfaced in the worker roster — subagents run headless and "cannot pause for interactive
per-tool approval"; delegation = delegated authority with scope restriction + read-only
specialists as the mitigation (same page). **INFERRED**: omp's answer to blocked-detection
is structural (read-only roles, budgets) rather than signal-based; 018's hook-driven
blocked state + native permission bridge is the stronger story for interactive TUI workers.

### Prompt delivery and follow-ups

No keystrokes anywhere (in-process sessions): steering a running worker lands "at a safe
step boundary"; messaging an idle worker starts a follow-up; messaging a parked worker
revives first; Esc/← returns without interrupting (**FROM-DOCS** subagents page).
Child-to-child messaging: `send` is fire-and-forget over the process-global bus; parked
recipients revive via lifecycle manager, idle ones wake with a real turn, busy ones get a
non-interrupting aside at the next step boundary (`AgentSession.deliverIrcMessage`);
per-agent mailbox cap 100 (oldest dropped); replies are real turns observed with `wait`
(or `await:true` send sugar); `inbox` drains pending; `list` shows actionable threads
(**VERIFIED-IN-SOURCE** `oh-my-pi/packages/coding-agent/src/irc/bus.ts`,
`irc/messaging.ts`). Hard boundary, verified upstream: bus + registry are **process-global
singletons** — two main agents in separate processes/tabs/worktrees **cannot** address each
other; multi-session-in-one-process has wrong-session/scoped-nowhere failure modes
(**FROM-DOCS** issues #7537, #10229, feature request #8077 citing Claude Code's
cross-session messaging v2.1.224+ as the missing capability). **This is the single most
relevant finding for signaltty**: in-process messaging does not cross the process/PTY
boundary that defines 018 — 018's server-owned events + future inbox are the correct layer.

### Output reading

Full child output reachable as `agent://` references; opening a worker shows live/persisted
transcript + current tool activity (**FROM-DOCS** subagents page). Structured by construction
(sessions, not screens) — no scraping problem exists in-process.

### Structured results and parent/child

`outputSchema` output contract (above) + per-worker usage (tokens/requests/tool calls/cost)
in the roster; inspector shows lineage (flat ↔ parent/child tree toggle), patch/branch
integration metadata (**FROM-DOCS** subagents page). Parent/child depth bounded by
`task.maxRecursionDepth` = 2 (**FROM-DOCS** settings table; `canSpawnAtDepth` in
`irc/messaging.ts`).

### Planning / spec-first

`/plan`: read-only planning turn on a dedicated `plan` model; full-screen Plan Review
surface (annotate `a`, delete section `d`, undo `u`, external-editor edit, model picker
via `continue with`); five approval exits controlling execution+context inheritance:
execute-fresh (plan only, max context) / compact / keep-full-context / refine / save-to-file
for later handoff (**FROM-DOCS** https://omp.sh/docs/plan). Script-defined orchestration
exists as community plugin (`zerx-lab/omp-dynamic-workflows`: `agent()/parallel()/pipeline()`,
phases, retries, quality gates — **FROM-DOCS** search result).

### Review / merge / PR / CI

No merge/PR/CI flow: integration = patch-apply or branch-merge into the parent checkout by
the main session, which "integrates and verifies". Review is a specialist role (`reviewer`,
evidence-backed) or a follow-up pass.

### Concurrency & cost limits

`task.maxConcurrency` = 32 (queue beyond), `maxRecursionDepth` = 2, `softRequestBudget` =
200 req, `maxRuntimeMs` = 0 (none), `agentIdleTtlMs` = 7 min → park, per-worker + aggregate
token/request/cost telemetry live in the Hub (**FROM-DOCS** settings table). Model routing
per role (everyday/fast/reasoning/planning + per-agent overrides) + advisor/prewalk calls
that add their own cost (same page).

### The UX that makes it rank high

Zero-friction in-process delegation ("Parallelize this migration…"), live Agent Hub roster
(status/assignment/model/age/tokens/cost) with transcript drill-down, steer-with-a-sentence,
sessions-as-branches (`/branch` rewind-to-message, `/fork` copy-on-write session, `/move`
across projects, HTML export), plan-review surface. Rank comes from **orchestration with no
plumbing**: no worktrees to manage, no CLIs to launch, no hooks to install — at the price of
process-locality.

### Distinctive features worth stealing

1. **`outputSchema` + exactly-one verbatim retry** ("more retries make frontier models drop
fields that were right") — adopt the one-retry rule for any future 018 result-schema
validation (deferred MCP tools).
2. **Steering-at-step-boundary / follow-up / revive-first** as the three-message semantic —
cleaner than busy/refused dichotomies for a future inbox; document as inbox semantics later.
3. **Patch-artifacts + nested-repo patches + `.retained-*` rescue dirs** — the failure-path
hygiene 018's `finish_error`/`cleanup_error` should match (name the rescued path in the error).
4. **Plan Review's five approval exits** (fresh/compact/keep/refine/save-to-file) — the
context-inheritance control 019's plan-first flow should copy verbatim.
5. **Per-worker live cost telemetry in the roster** — the argument for 019 cost budgets, and
the shape (tokens/reqs/cost per task) to spec.
6. **Keep-alive pools with queued-item states** — only if 018 ever needs aggregate fan-out
batches; not now.

---

## 3. Traycer (traycer.ai) — plan-first outer loop + agent-to-agent Task tree

### Architecture & stack

Open-source (MIT) desktop app + IDE extension (separate product); BYO provider
subscriptions or native inference; Chat interface (any coding agent) + Terminal interface
(PTY: Claude/Codex/OpenCode only); plain-shell Terminals as a separate surface; multi-host
+ cloud relay delivery; cross-device sync
(**FROM-DOCS** https://docs.traycer.ai/, README). Self-description: "the **outer-loop
Agent** that coordinates specialists + inner-loop code-gen agents" (planning, decomposing,
context-gathering, verifying — not writing the next 30 lines)
(**FROM-DOCS** https://traycer.ai/blog/inside-traycers-multi-model-architecture).
Protocol packages in-repo (`protocol/`: zod RPC contracts incl. `host/agent/inbox.ts`,
`worktree/classify-worktree.ts`).

### Task / work-unit model and states

**Task** = top-level container: related agents, panels, terminals, files, git diff,
durable **artifacts** (specs, tickets, stories, reviews as markdown files written by agents
via built-in skills) (**FROM-DOCS** `concepts/artifact-workflow.mdx`, docs index).
Plan→build pipeline of skills: `epic-brief → core-flows → tech-plan → ticket-breakdown →
execute → review → walkthrough`, plus `autobuild` (spec + grading rubric + builder/evaluator
loop), `debate` (multi-agent structured debate → synthesis), `artifact-critique`,
`revise-requirements`, `housekeeping` (same file). Execution skill `traycer-execute`:
**orders tickets into batches by dependency, starts one child agent per ticket, reviews each
batch against the plan**; pass → done, small problem → fix-up ticket, product drift → back
to user (same file). Ticket states live in the artifact set; agent hierarchy renders as a
tree (child under creator; cross-Task children at the target Task's top level).

### Agent spawn + isolation + base ref + cleanup

Per-workspace-folder **run locations**: Local (in place) / New worktree (new branch, source
∈ {working-tree-with-uncommitted-changes-carried-over, current branch, other local branch,
remote branch}, generated `prefix/adjective-noun` names, collision-safe) / Existing worktree
(as-is); Terminal-interface agents fix location at launch, Chat agents can switch per turn;
setup/teardown scripts per repo in `.traycer/environment.json` with setup-card states
(Creating → Setting up → Ready / Failed / Cancelled / Creation-failed + Retry)
(**FROM-DOCS** `concepts/worktrees.mdx`). Home: `~/.traycer/worktrees/<owner>__<repo>/<slug>`
(same file) — same shape as 018's data-dir default (validates FR-011's path choice).
Cleanup = **Sweep**: per-Task(s) worktree removal with landedness evidence per row
(Landed / At-base-commit / Review-not-proven / shared-outside-sweep / In-use),
pre-checked only when safe; removes folders (incl. uncommitted+ignored) but **keeps
branches**; closes terminals inside; teardown scripts run; auto-cleanup skips in-use
(**FROM-DOCS** `concepts/sweep.mdx`). Removing ≠ deleting Task (conversations+artifacts
kept). Matches 018's keep-branch default and `PANES_ALIVE` refusal.

### Readiness / turn-completion / blocked detection

No hook-protocol detail found in docs; Terminal Agents store the upstream coding-agent
session id and resume it after app/host restart on the original host (**FROM-DOCS**
`concepts/terminal-agents-vs-terminals.mdx`). Needs-attention surfaces via notifications
concept page + review/verification queues rather than a ranked pane list. **INFERRED**:
readiness intelligence lives in the Chat-agent layer (Traycer-driven sessions), while PTY
agents are treated as wrapped upstream sessions — 018's hooks-first lifecycle for PTY
workers goes further than Traycer documents for its Terminal interface.

### Prompt delivery and follow-ups

**Agent-to-agent messaging with three separated capabilities** — reference (always, any
agent in Task) vs transcript-read vs message-receive (narrowest)
(**FROM-DOCS** `concepts/agent-to-agent.mdx`). Inbox availability matrix
(**VERIFIED-IN-SOURCE** docs + protocol): Chat interface = every agent sends+receives;
Terminal interface = only Claude Code receives (Codex/OpenCode Terminal agents send-only:
fire-and-forget + read results from artifacts/transcripts; **cannot be created by another
agent** since the brief could never arrive). Cross-Task + cross-host delivery via cloud
(send succeeds on cloud accept). Reject (not queue) when: no inbox, foreign user, revoked
host, reply-requested-but-sender-has-no-inbox, >16 MB. Reply returns to sender's inbox in
sender's Task. Transport: `agent.sendMessage` fire-and-forget enqueue onto a RAM-only
per-receiver broker queue; `traycer monitor` streams it into a Claude Code TUI session;
backlog replays on connect; `expectReply=true` arms an inactivity-sweep stalled-receiver
notice; role-awareness broadcasts are never queued (stale = dropped)
(**VERIFIED-IN-SOURCE** `traycerai_traycer/protocol/src/host/agent/inbox.ts`,
`host/agent/shared.ts` `expectReply` thread tracking). Agent selection (coding agent +
model + effort) via global defaults refinable per workspace (`.traycer/agent-selection-guide.md`);
agents can claim **roles** for visible ownership (same agent-to-agent page).

### Output reading

"Durability Is Not Availability": Terminal-agent transcripts are read from the coding
agent's **own session history, not terminal scrollback** — survives terminal/app close;
cross-host reads proxy to the owning host; shared-Task reads are read-only Chat transcripts
(same agent-to-agent page). Same transcript-over-scrollback hierarchy as Superset.

### Structured results and parent/child

Artifacts ARE the result channel (spec/ticket/story/review files + agent hierarchy tree +
Agent office message log). Parent = creator; child gets own session/model/transcript/run
state, never merges into parent; parent reads result, continues, or uses as next-step
context (same page). Roles (`claim a role`) = lightweight ownership labels.

### Planning / spec-first flows (how plans become worker tasks and are verified)

Two modes (**FROM-DOCS** extension docs): **Plan mode** (single-PR: query → file-level plan
with symbol references → iterate → execute in agent → verify → ship) and **Phases mode**
(complex: query → optional intent-clarification questions → phase generation (milestones,
order, boundaries; drag-drop reorder, insert, multi-select-merge) → per-phase detailed plan
(objectives, deliverables, **file changes with exact edits**, architecture) → hand off to
agent → **verify each phase before advancing**, context carried forward
(`extension/tasks/phases.mdx`, `plan.mdx`)). **Verification**: implementation-vs-plan diff
review producing Critical/Major/Minor/Outdated comments; fix-one / fix-selected / fix-all
dispatched back to the coding agent; Re-verify (focused) vs Fresh verification (full)
(`extension/tasks/verification.mdx`). **Review mode**: agentic deep-exploration review with
Bug/Performance/Security/Clarity categories (`review.mdx`). Ensemble behind it: Sonnet-4.5
plans/decomposes, GPT-5.1 verifies/critiques, Grok-fast scouts gather, mini-models
summarize, parallel.ai searches; thinking/gathering strictly separated ("scouts don't
editorialize") (multi-model blog post). YOLO mode = fully automated plan→code→verify loop
(`phases.mdx`). **For 018**: plan→tickets→batched-children→per-batch-review is the
reference refinement loop; 018's contract (objective/constraints/acceptance/output format)
is the per-ticket payload, and verification-severity triage is what an evaluator loop
(019) should emit.

### Review / merge / PR / CI

Git Diff panel per worktree; Sweep shows merge progress ("merged 1/2") and Landed evidence
(merged PR matching HEAD / commits contained in default branch). No auto-merge documented;
integration stays user-driven.

### Concurrency & cost limits

Per-Task usage control shown in the status row (Sweep entry point sits next to it);
per-agent model/effort selection bounds spend; no explicit worker-cap number found in docs
read. (**INFERRED**: cost control via model routing + effort levels, like omp.)

### The UX that makes it rank high

Intent-preserving workspace: Tasks + artifacts + agents + files + diff in one surface;
phases you can reorder/insert/merge; verification comments routable to agents at any
granularity; multi-model ensemble invisible to the user; BYO subscriptions; real-time
collaboration (shared boards, assignment); cross-device sync. Rank comes from **never
losing intent, context, or visibility** — the plan is a living artifact, not a prompt.

### Distinctive features worth stealing

1. **`traycer-execute` batch loop** (dependency-batched children + per-batch plan review +
fix-up tickets + drift-stop) — the exact evaluator pattern 019 should implement on top of
018 primitives; 018 needs no change to support it (tickets = tasks in one context).
2. **Three-capability split (reference / read / receive) + explicit reject-not-queue rules**
(>16 MB, no-inbox, foreign-user, reply-without-inbox) — adopt as the future inbox's
delivery contract (mirrors cmux fail-open + Superset envelope limits).
3. **Stalled-receiver notice on `expectReply`** (inactivity sweep) — the 018 `TIMEOUT` analog
for a future inbox wait; note the asymmetry (RAM-only queue, never-queue-awareness).
4. **Sweep landedness evidence** (Landed / At-base / Review / shared / In-use with
pre-checks) — richer than 018's finish guards; candidate `task.sweep`-style preview for 019.
5. **Working-tree-carryover as an explicit source option** (vs 018's refuse-dirt) — keep
018's refusal, but steal the explicitness: name the choice at start.
6. **Claimable roles** — 30-minute version of 018's labels for ownership display in GUI rows.
7. **Scouts-don't-editorialize** (thinking/gathering separation) — guidance for orchestrator-
agent prompts, not server code.

---

## 4. Hermes agent (Nous Research) — delegation contracts + kanban dispatcher + worktree lanes

### Architecture & stack

Provider-agnostic Python agent framework (20+ providers, credential pools, mid-workflow
model swap); surfaces: CLI, Ink TUI, Electron desktop, web dashboard, **ACP server** for
IDEs, OpenAI-compatible proxy, messaging gateway (Telegram/Discord/Slack/…), profiles
(isolated configs/sessions/skills/memory); plugin-safe APIs; skills incl. bundled
orchestration skills (**FROM-DOCS** skill hub `skills/autonomous-ai-agents/hermes-agent/SKILL.md`
v3.2.0). Sessions in SQLite+FTS5 (`~/.hermes/state.db`) + JSONL transcripts; per-profile
homes. Cross-profile orchestration over ACP exists as a proposed PR
(`scripts/hermes-session-client.py` reference client: create/resume/fork/prompt/list/cancel
across profiles — **FROM-DOCS** PR #14009 title/description), i.e. the same direction as
018's future (typed orchestration surface) but over ACP between profiles, not over PTYs.

### Task / work-unit model and states

Two layers. (a) **`delegate_task` subagents**: goal + context + role + model + toolsets;
states PENDING → STARTING → RUNNING → SUCCEEDED | FAILED | INTERRUPTED |
CANCEL_REQUESTED → CANCELLED (+ UNKNOWN); launch/status/wait(timeout)/cancel/result/
reconnect API with HMAC-capability handles, idempotency via per-parent `correlation_id`
(duplicate rejected), terminal-result retention 1h, shape-checked handles, parent-match
enforcement (**VERIFIED-IN-SOURCE** `NousResearch_hermes-agent/agent/subagent_lifecycle.py`,
416 lines). (b) **Kanban boards**: dispatcher owns cards; worker profiles see focused
`kanban_*` toolsets (`show/complete/block/heartbeat/comment/create/link`) gated by
`HERMES_KANBAN_*` env; orchestrator profiles may take the broad `kanban` toolset
(`+ list/unblock`); gateway dispatcher reclaims stale claims, promotes ready tasks,
atomically claims, auto-blocks after `failure_limit` consecutive spawn failures (default 2)
(**FROM-DOCS** `background-systems.md`). Results: `SubagentResult` = summary (≤32k) +
`structured_payload` + usage + tool-execution summary + error classification + **sha256
result hash** over the payload (lifecycle file). `output_schema` (JSON Schema) supported
per delegation: OUTPUT-CONTRACT block appended to child context, parent validates with
jsonschema, **exactly ONE bounded retry** with verbatim errors ("more retries make frontier
models drop fields that were right") (**VERIFIED-IN-SOURCE**
`NousResearch_hermes-agent/tools/delegation_output_schema.py`).

### Agent spawn + isolation + base ref + cleanup

`delegate_task`: single / batch-parallel (capped `max_concurrent_children`, default 3, hard
ceiling warn >10) / background (handle now, result re-enters as a new turn); roles
`leaf` (default, cannot re-delegate) vs `orchestrator` (may spawn workers, bounded by
`max_spawn_depth`, flat default depth 1); **NOT durable** — process-local, lost on parent
exit; durable work goes to `cronjob` or background `terminal(notify_on_complete=True)`
(**FROM-DOCS** `background-systems.md`; **VERIFIED-IN-SOURCE**
`tools/delegate_tool_config.py` `MAX_DEPTH = 1`). Opt-in `delegation.worktree_isolation`:
one worktree per child at `<repo>/.worktrees/subagent-<id>`, branch `hermes-subagent/<id>`;
**pruned only on proof (zero commits AND clean)** else kept + `inspection_failed`; git-only,
local-backend-only, noninteractive git env (GHSA-hardened), hooks-aware
(**VERIFIED-IN-SOURCE** `NousResearch_hermes-agent/tools/subagent_worktree.py` module
docstring + helpers). Long missions: spawn full `hermes` subprocesses (one-shot `-q`,
interactive PTY via tmux with `capture-pane`/`send-keys` choreography, `-w` worktree mode
to avoid conflicts, session `--continue/--resume`); scheduled work via `cronjob` tool
(**FROM-DOCS** SKILL.md spawning section). Concurrency diagnosis doc names three real cap
paths (else "the model is self-limiting") — dominance of caps as a theme.

### Readiness / turn-completion / blocked detection

`wait(handle, timeout)` on Futures; background children re-enter the parent loop as a new
turn on completion; cancellation via hard-interrupt request; `reconnect` explicitly reports
RECONNECT_UNAVAILABLE after restart rather than relaunching work (**VERIFIED-IN-SOURCE**
lifecycle file) — the honest-restart rule, same family as 018's failed-with-evidence.
Kanban liveness: worker **heartbeats** + dispatcher stale-claim reclamation (background-
systems.md). Blocked = first-class card state (`kanban_block`) with reason — the only Tier-B
tool with an explicit blocked primitive on the durable object (vs hook-derived `blocked` in
018). Capability narrowing: children get a subset of parent toolsets, never broader; unsafe
child tools always blocked; `working_directory` per-launch and per-tool blocking both
**rejected by design** ("isolated task environments", "use allowed_toolsets")
(lifecycle `_REQUEST_REJECTIONS`).

### Prompt delivery and follow-ups

In-process (no keystrokes): single `delegate_task(goal, context)`, batch `tasks=[...]`,
`background=true` handle; follow-ups flow through re-entry turns. Cross-process follow-ups
use tmux send-keys or `hermes chat -q` one-shots (SKILL.md). `delegation_context.py`
(ContextVar child markers, kanban env pins, fail-closed identity gates) keeps dispatcher
vs child vs cron identity separate (**VERIFIED-IN-SOURCE**
`NousResearch_hermes-agent/agent/delegation_context.py`).

### Output reading

Session transcripts (SQLite/JSONL), live delegation log (`tools/delegation_live_log.py`),
progress API (`delegate_tool_progress.py`); no PTY layer in the delegation path.

### Structured results and parent/child

Result hash (tamper-evidence), usage metadata, tool-execution summary, error
classification; parent binding via weakref context (`bind_subagent_parent` — avoids pinning
finished children in heap; lifecycle file). Roles bound recursion; correlation ids make
parent-side retries idempotent.

### Planning / spec-first

`dynamic-workflow` skill (PR #37272): plan-in-code fan-out with built-in verification,
ported from Claude Code's "dynamic workflows" (Opus 4.8, May 2026) — **move the plan/loop/
intermediate state out of the context window into a script** so context holds only the
final verified answer (**FROM-DOCS** PR description). Kanban cards carry goal mode +
max turns (`HERMES_KANBAN_GOAL_*`). Controller/worker skills (`kanban-orchestrator`,
`kanban-worker`) ship in hermes-conductor (§5), not core.

### Review / merge / PR / CI

Core has no merge/PR flow (delegation returns results; kanban cards complete with evidence).
Verification thinking lives in the ecosystem (§5).

### Concurrency & cost limits

`max_concurrent_children` (3, warn >10), `max_spawn_depth`, background-capacity equality,
three documented cap paths, credential-pool rotation across keys; per-result `api_calls`
accounting. No USD budgets in core (executor receipts with `max_budget_usd` are a
hermes-conductor-lane innovation, §5).

### The UX that makes it rank high

One brain, every surface (terminal/desktop/chat-ops from Telegram while it runs on a VPS);
self-improving skills loop; profiles; provider freedom with mid-flow swaps; kanban as the
durable multi-day mission backplane (cron + dispatcher + heartbeats). Rank comes from
**durability + breadth of embodiment**, not from PTY orchestration.

### Distinctive features worth stealing

1. **HMAC-capability handles + `correlation_id` idempotency + shape-checked deserialization**
— the exact hardening 018's opaque `task_…` ids + per-path lock + double-finish refusal
gesture at; adopt the vocabulary (duplicate-correlation → `BAD_PARAMS`) if 018 ever
exposes idempotency keys.
2. **`reconnect` that honestly reports RECONNECT_UNAVAILABLE** — matches 018's
restart→failed-with-evidence; keep.
3. **Prune-only-on-proof worktree cleanup** (zero commits AND clean, else kept +
named failure) — stricter and better-evidenced than 018's discard; consider for
`task.finish` failure detail.
4. **Exactly-one verbatim validation retry** (independent convergence with omp) — adopt for
future result-schema validation.
5. **Result hash over the payload** — cheap tamper-evidence for `task.result`; 019-candidate
audit field.
6. **Blocked as a durable card state with reason + heartbeat liveness** — 018 derives
blocked from hooks; a heartbeat/claim-expiry for `working` tasks would close the
"hook never fires" hole (see Deltas).
7. **Leaf vs orchestrator roles + flat-by-default depth** — 018's `relationship`
(fork|subagent) covers lineage but not re-delegation depth; consider a depth bound.

---

## 5. Ecosystem note: hermes-conductor (third-party) — "zero trust in self-reports"

`forcewake/hermes-conductor` is NOT Nous Research; it is production incident lore (18 boards,
367 cards, 566 dispatches) encoded as 7 patterns + 2 skills. Included because its golden
rules are the sharpest independent validation of 018's design — and its gaps point at 019.

- **Controller contract** (pattern 01): card stays blocked/controller-owned until worktree +
launch command verified; fresh worktree from verified canonical branch; prompt file with
scope/forbidden-scope/tests/evidence path + `do not push/merge`; agent report untrusted
until controller verifies git state + gates (**VERIFIED-IN-SOURCE** `/tmp/hc/patterns/
01-controller-managed-external-worktree-lanes.md`).
- **Verification gates**: `status --short --branch`, `log --oneline --decorate`, `diff --stat
BASE...HEAD`, then targeted tests + one canonical integration gate; completion requires
artifacts-exist + lane-gates-pass + integrated + canonical-gates-pass + commit IDs and
command output on the card (same file). Lane executors: raw CLI (`claude -p`) or SDK runner
with per-lane `max_budget_usd`, `max_turns`, hard timeout, explicit tool grants,
fail-closed provenance checks (`expected_model`/`expected_context_window`), **JSON receipts
(turns/cost/result)** attached as evidence instead of parsed chat (README).
- **Failure playbooks that match 018's edge list**: same-checkout collision → reclaim +
controller-sequential batches (pattern 05); stale-base auto-promotion → block/reclaim,
`git worktree list` audit, relaunch from canonical HEAD (pattern 01); approval-deadlock
respawn loops; evidence-free "done" (README table). "Completing a dependency may
auto-promote stale lanes" is the exact hazard behind 018's recorded-`target_branch` +
checkout-equality check (FR-024).
- Golden rules most relevant to 018: orchestrator never implements; every mutating lane gets
its own worktree; `--parent` is a dependency edge, not a folder (children blocked until
parent completes **with evidence**); controller-sequential for dependent chains unless the
plan proves independence.

---

## 6. Cross-tool table (what 018 keeps / what is new)

| Dimension | 018 already has (via external/practices) | Tier B adds |
| --- | --- | --- |
| Hook normalization | herdr refuse/gate; workmux map | Superset's never-default-Stop + harness-disagreement-drop + subagent-roster split |
| Readiness without hooks | wait-baseline + timeouts | Hermes heartbeat/claim-expiry (durable liveness); omp budgets (turn yield) |
| Submit gate | bracketed paste + Enter + 5 s activity gate | omp's step-boundary steering semantics (future inbox); Traycer reject-not-queue rules |
| Incremental read | content_seq cursor (nobody else has it) | transcript-over-scrollback hierarchy (Superset `agents read`, Traycer session history) as documented fallback order |
| Result channel | `task.report` + event; inbox deferred | Superset envelope-as-anti-pattern (why the event must exist); Hermes result-hash; omp/Traycer schema-retry-once |
| Worktree policy | base SHA, per-path lock, keep-branch, clean-target | Traycer Sweep evidence rows; Hermes prune-only-on-proof; omp `.retained-*` rescue naming |
| Plan-first | contract per task (FR-010) | Traycer execute-batch loop; omp Plan Review exits; Hermes plan-in-code (dynamic-workflow) |
| Review/merge | workmux rules + PR-per-worker path | Superset PR view + comment-to-agent; conductor verification-gate checklist + JSON receipts |
| Concurrency/cost | cap 4 (RATE_LIMITED) | omp's telemetry shape (tokens/reqs/cost per worker); conductor per-lane budgets; Hermes 3-path cap diagnosis |
| Crash honesty | failed-with-evidence, checkout preserved | Hermes RECONNECT_UNAVAILABLE wording; omp non-revivable-isolated-workers rule |

---

## Deltas for 018

Ranked by value/effort. All are additive clarifications to `spec.md` / `contracts/ipc.md` /
`data-model.md`; none changes the task lifecycle, IPC shapes, or deferred scope.

1. **Hook-event receiver: never-default-to-done + harness-disagreement-drop** (spec FR-006,
contracts `hook-event` section). Superset proves two receiver bugs concretely: defaulting an
unparseable event to Stop (false completion) and cross-harness replay (Claude config firing
inside a Cursor/Codex session) relabeling the wrong pane. Spec: unparseable → drop with
counter; wrapper-identity vs hook-harness mismatch → drop, never relabel. Effort: small,
testable with synthetic events.
2. **Subagent events → roster only, never lifecycle** (FR-006/FR-023). cmux + t3code + Superset
+ workmux (`SubagentStop → working`) now agree 4-ways. Make explicit: `SubagentStop` keeps
task `working`; subagent presence is recorded (worker roster fields already exist) but never
moves the parent task. Effort: one paragraph + test.
3. **Liveness backstop for `working` tasks: hook-watchdog timeout** (FR-019/data-model
`status_reason`). Every Tier-B tool assumes hooks can go missing (Superset completion-only
agents; Hermes heartbeats + claim expiry; Codex `exec` hook gaps in practices §2.3). Add:
a task that stays `working` with no hook activity and no output for N (configurable, default
e.g. 10 min) transitions to `input_required` with `{reason: "worker_silent", …}` instead of
hanging the settled-wait loop. Small, no new IPC; documents the degraded mode practices
already demands.
4. **Document the read hierarchy** (FR-001/FR-002): saved transcript (when providers expose
it) > rendered grid + `content_seq` > raw bytes; alt-screen already documented. One paragraph
in spec + contracts; kills future "why not parse the JSONL" debates with three citations
(Superset `agents read`, Traycer durability-note, practices §2.3 fragility).
5. **`task.finish` failure detail: name the rescue path** (contracts `task.finish`,
data-model `finish_error`). omp `.retained-*` + Hermes `inspection_failed` + workmux
conflicted-files converge: every finish failure must name what was kept and where
(worktree path, branch, conflicted files). Already mostly there via `details.conflicted[]`;
extend to cleanup-failure (`cleanup_error` gains kept-path). Effort: tiny.
6. **Restart wording + non-revivable rule** (FR-019, edge cases): adopt Hermes'
RECONNECT_UNAVAILABLE honesty explicitly for `task.wait` reattach (waiters reconnect and
re-read; no resume of the dead worker), and omp's rule that post-teardown workers are never
revived (transcript + metadata stay). Mostly documentation of existing behavior.
7. **Blocked evidence should carry the permission/question text** (contracts
`attention.pending` `last_message?`, FR-008). Superset's inline prompts + Traycer's
message-reject reasons show the orchestrator needs the *content* of the block, not just the
state. Ensure hook `PermissionRequest`/`Notification` message text flows into `last_message`
(small pipe-through if not already specified).
8. **No change**: keep envelope-free results (Superset proves prompt-convention envelopes
without events rot); keep keep-branch default (Traycer Sweep + Hermes prune-on-proof agree);
keep local-HEAD default base (only Conductor fetches; Tier B adds no new evidence);
keep cap-4 default (omp 32 is in-process; Hermes 3 is closest external analog — 4 sits
right between, no reason to move).

## Ideas for 019 board/PR-CI follow-up

(seed list; each is a separate follow-up, none in 018 scope)

1. **Evaluator loop (`traycer-execute` pattern)**: dependency-batched children, per-batch
plan-vs-diff review emitting Critical/Major/Minor, fix-up tasks for small drift, drift-stop
to user. Builds purely on 018 primitives (tasks-in-context + contracts + diffs).
2. **Verification-gate checklist + JSON receipts** (hermes-conductor): controller re-runs
targeted tests + canonical gate, attaches commit IDs + command output as result evidence;
per-lane budgets (`max_budget_usd`, `max_turns`, hard timeout). Needs 019 cost telemetry.
3. **Sweep-style landedness preview**: Landed / At-base / Review-not-proven / shared /
In-use rows with safe-pre-checks before bulk `task.finish` — the multi-task review UX.
4. **Comment-to-agent from diff**: selected diff lines → worker follow-up (Superset PR view);
pairs with 018's `pane.submit` follow-up path (FR-023).
5. **Run-summary page**: one persistent link per multi-task run (outcomes, branches, checks),
commentable per row (Superset Pages).
6. **Plan Review approval exits** (omp five choices: fresh/compact/keep/refine/save-to-file)
for a plan-first front-end; plan-in-code (Hermes dynamic-workflow) as the screen-less
variant. Plan artifact = 018 contract at larger scope.
7. **Per-task cost telemetry** (omp Hub shape: tokens/requests/cost per worker, aggregate)
as the precondition for cost budgets (deferred in 018).
8. **Result-hash audit field** on `task.report` (Hermes sha256-over-payload) + claimable
roles for GUI rows (Traycer) — small, high-trust additions.
9. **`task.handoff`** (Superset continue-with-another-agent: transcript slice + `git status`
grounding + credential warning) — worker-to-worker context transfer without re-planning.
10. **Future inbox delivery contract** (deferred in 018): Traycer reject-not-queue rules +
stalled-receiver notice, omp step-boundary/in idle-aside/revive-first semantics, cmux
fencing/headless-exclusion from external research — the inbox spec is now fully sourced
before it is written.
