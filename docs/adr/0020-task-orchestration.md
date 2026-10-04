# ADR-0020: Task orchestration

Status: accepted, 2026-10-04 (spec author proposal; ratified on PR merge).

## Context

An agent in a pane must fan out to worker agents (spawn in isolated worktrees,
reliable prompts, readiness/completion, incremental reads, structured results,
review → merge/discard) surviving restart. Evidence: `specs/018-orchestrator/
research-{codebase,external,practices}.md`. What exists: hooks-first lifecycle,
`wait` with identity baselines, `worktree.*` refusing dirty trees, `pane.read`
(tail garbles TUIs; zero scrollback), raw `pane.input` (`\n` does not submit in
raw-mode TUIs), no parentage/labels, no task entity, needs-user limited to one
pane via `focus.next_unread`.

## Decision

Persist a `Task` with an A2A-aligned lifecycle
(`pending → working ⇄ input_required → completed|failed|canceled|rejected`,
terminal immutable, refinements as new tasks in the same context;
VERIFIED https://a2a-protocol.org/dev/topics/life-of-a-task/) carrying a
contract (objective/constraints/acceptance/output format — vague briefs are
Anthropics #1 delegation failure;
VERIFIED https://www.anthropic.com/engineering/multi-agent-research-system/).

- Submit like herdr (`src/app/api/agents.rs`): refuse blocked/not-ready before
  any write, bracketed paste + separate delayed Enter, 5 s activity gate on the
  existing `WaitBaseline`. New codes `AGENT_BUSY`/`AGENT_NOT_READY` keep the
  transient-vs-inspect split; stall reuses `TIMEOUT` with stage detail.
- Read rendered text with the cursor nobody else has (herdr `revision` is
  hardcoded 0, `src/app/api/panes.rs`): monotonic `content_seq`, dropped-range
  signal, 5000-line ring; `tail` rebased on the grid (root fix for
  research-codebase §1). Reads stay passive — no herdr-style `agent_not_idle`
  refusal, which only makes sense for viewport-moving reads.
- Report results via `task.report` (`signaltty report`), stored + emitted;
  parents collect via `task.wait`/`task.get` (herdr/workmux wait-and-re-read).
  cmux-style injection (`AgentMessage.swift`) is deferred: correct direction,
  second PR.
- Finish = explicit merge/discard with workmux rules (`src/workflow/merge.rs`):
  refuse dirt, abort conflicts leaving the target clean, cleanup errors separate
  from merge success. Diff vs recorded base SHA (claude-squad `BaseCommitSHA`,
  t3code checkpoint 0), never `git add -N` as a read side effect.
- Dead pane / restart + non-terminal task → failed with evidence, checkout
  preserved (claude-squad pause-the-worktree, `session/instance.go`); completed
  tasks + results survive intact.
- Lineage (`parent/root/relationship fork|subagent`, label) at spawn (t3code
  `orchestrationV2.ts` lineage); `$SIGNALTTY_PANE` unchanged.
- Cap parallel tasks (default 4 — "lead spawns 3–5 workers",
  VERIFIED anthropic multi-agent post) → `RATE_LIMITED`; cost budgets deferred
  (no telemetry; transcripts vendor-warned unstable).
- MCP server deferred (constitution VII + docs/14 composable primitive); IPC/CLI
  maps 1:1 onto future tools. GUI shows label + state on existing rows only.

## Disagreements resolved (research vs research)

- Branch after merge: workmux deletes, t3code keeps for resume → keep by
  default, explicit flag deletes, pre-existing never (durable records favor
  post-merge inspection over tidiness).
- Round-1 review: synchronous `task.start` (block up to ~35 s on ready-wait) →
  async. Sync serialises fan-out (3 starts = 3 sequential ready-waits) and
  diverges from A2A (the task object is returned immediately; progress is
  observed via state). Sync cap → base → worktree → spawn stays (fast, bounded;
  cap refusal is still a synchronous `RATE_LIMITED` with nothing created, so
  backpressure is kept); only the ready-wait + gated submit move to a
  server-owned background step, observed via `task.wait --until working`.
- Round-1 review: a worker turn ending (`done`) with no report left the task in
  `working` forever → A2A `input_required` (interrupted) with
  `turn_ended_without_report` evidence; the requester follows up or
  cancels/discards. `task.wait` defaults to `settled` (terminal OR
  input_required) so the loop cannot sleep through it.
- Round-1 review: default worktree path `<repo>/../<repo>-<branch>` →
  `$XDG_DATA_HOME/signaltty/worktrees/<repo-dirname>-<short-hash>/
  <sanitized-branch>` (t3code `~/.t3/worktrees`, Conductor
  `~/conductor/workspaces`; t3code `GitVcsDriverCore.createWorktree` default
  `worktreesDir/repoBasename/sanitizedBranch`, research-external §t3code).
  `git worktree list` shows every registration regardless of path, so
  discoverability is kept; explicit `path` still wins; cleanup stays guarded
  to the recorded path.
- Round-1 review: merge target inferred from the source checkout at finish time
  → recorded `target_branch` at start, with a checkout-equality check at
  finish (`BAD_PARAMS` `{expected, actual}`). Finish must never merge into
  whatever branch the user happened to switch to since start.
- Restart with pending result: t3code forces back to working → we fail with
  evidence (our PTYs are dead after restart per docs/09; working-with-no-process
  would be limbo). A task still `pending` at restart (background ready-wait died
  with the server) fails with `{stage: "restart"}`.
- Blocked+input-required as "ready" (research-codebase §2) vs herdr
  refuse-blocked → submit refuses blocked (prompts ≠ answers); answers use
  `decision.answer`/`pane.input`, so a fresh prompt can never corrupt a
  permission dialog (cmux draft/dialog guard, `docs/cli-contract.md`).
- Staged changes on merge: workmux commits via editor → we refuse (non-
  interactive server must not invent authorship; commit explicitly first).
- Base ref: Conductor always-fetch vs local HEAD → default local `HEAD`, opt-in
  `fetch_first` (offline-capable default for a local multiplexer).
- Evaluator self-check (research-practices #14) → deferred: the server stays an
  agent-agnostic multiplexer; the orchestrator agent evaluates against contract
  acceptance criteria.

## Implementation amendments (lane C record, 2026-10-04)

What the code does where it diverges from the proposal above — docs
(`docs/02`, `docs/08`, skill, quickstart) describe this behavior:

- No `client_request_id`: neither the spec contracts nor the code define an
  idempotency key for `task.start`. Retries create new tasks; callers
  de-duplicate via `task.list --context`.
- Sync-error semantics: base-ref/empty-repo/worktree/spawn failures return
  synchronous errors (`BAD_PARAMS`/`IO_ERROR`) and create no task. Only the
  background ready-wait + prompt write produces `failed` tasks with `{stage:
  ready_timeout|submit_refused, …}`. The accepted-research deltas all
  landed: hook-receiver drops (`harness_mismatch`, `unrecognized_hook`),
  the silent-worker watchdog (`worker_silent_timeout_s`, default 600 s, `0`
  disables), `last_assistant_message` flowing into pane `last_message` and
  turn-end evidence, ref-safety checks, and the composed worker preamble.
- Background submit writes the prompt (`submit_delay_ms`, default 300, the
  same paste/Enter delay as `pane.submit`) and runs the `pane.submit`
  activity gate. A newer `working` or `blocked` transition moves the task to
  `working`; a stall fails it with `{stage: activity_gate}`.
- `pane.spawn` stores an unknown `parent_pane_id` as-is (root unset) rather
  than refusing `NO_SUCH_PANE`.
- `Task.finish_error` stores conflict files (no disposition, so finish can
  be retried) and `{cleanup_error}` when removal fails after a disposition
  is recorded. The RPC still returns `cleanup_error` inline. A second finish
  retries removal while the recorded path exists.
- Board / PR-CI cycle (Agent Orchestrator's Kanban of derived PR states,
  CI/review auto-paste, GitHub squash finish) is explicitly deferred to a
  follow-up feature (019): this PR merges locally into the recorded target
  branch with a human review gate, and surfaces label + state on existing GUI
  rows only.

## Consequences

New `Task` in core + `Snapshot.tasks` (serde-defaulted, old snapshots load);
`task.*`, `pane.submit`, `attention.pending` methods; `task.*` journaled events
with task-scoped replay; `NO_SUCH_TASK`/`AGENT_BUSY`/`AGENT_NOT_READY`/
`MERGE_CONFLICT` codes; CLI parity; GUI row chip. Round-1 amendments: async
`task.start` (`pending` → background ready + submit → `working`/`failed`), turn
end without report → `input_required`, `task.wait` default `settled`, data-dir
worktree root, recorded `target_branch` with checkout check. Follow-ups: parent inbox,
MCP server, cost budgets, kanban — each additive on this contract.
