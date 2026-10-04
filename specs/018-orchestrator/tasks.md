# Tasks: task orchestration

**Input**: `spec.md` (US1–US7), `plan.md`, `data-model.md`, `contracts/ipc.md`,
`quickstart.md`

**Tests**: TDD-first throughout — test tasks precede implementation in every
phase and MUST fail before the implementation lands.

**Lanes** (for parallel implementers; shared files are handed off, never
co-written): **A** server tasks (`store.rs`, `persist.rs`, `router.rs`
task handlers, `params.rs` task params) · **B** pty/term/submit
(`pty.rs`, `headless.rs`, `router.rs` pane handlers) · **C** git/finish
(`git.rs`, `file_diff.rs`, `worktrees.rs`, `paths.rs` data dir) · **D** CLI/GUI/docs
(`signaltty-cli`, `signaltty-gui`, docs). `router.rs`/`params.rs` are
single-writer: A and B sequence their edits (B's submit/reads first, then A's
task handlers, or vice versa — agree at phase start).

## Phase 1: Setup (shared fixtures + E2E skeleton)

- [x] T001 [US6] Write the deterministic E2E skeleton in
  `crates/signaltty-server/tests/orchestrator_e2e.rs` (temp repo → 3 async
  starts back-to-back → wait `working` → fake hooks → reports (worker C via
  the input_required leg: turn ends without reporting → settled wait ends →
  follow-up submit → report) → context wait → reads → restart →
  merge/discard asserts); MUST fail (no `task.*` methods yet). Disabled
  (`#[ignore]`) until Phase 8 greens it.
- [x] T002 [P] [US6] Add testkit fixtures in
  `crates/signaltty-testkit/src/lib.rs`: temp git repo helper, synthetic
  hook-event driver, server restart-on-same-socket/state helper.

**Checkpoint**: E2E skeleton compiles, fails honestly, documents the bar.

## Phase 2: Foundational (entity + protocol — blocks all stories)

- [x] T003 [P] Core `Task`/`TaskState`/transitions + contract/result validation
  (objective 1…32 KiB, summary …8 KiB, artifacts ≤ 32) with in-module unit
  tests in `crates/signaltty-core/src/model.rs` and
  `crates/signaltty-core/src/state.rs`.
- [x] T004 [P] Protocol constants in `crates/signaltty-proto/src/lib.rs`:
  `task.*` methods, `pane.submit`, `attention.pending`, `task.*` events +
  `task.*` glob, codes `NO_SUCH_TASK`, `AGENT_BUSY`, `AGENT_NOT_READY`,
  `MERGE_CONFLICT`; extend `ALL` lists + schema output.
- [x] T005 `Store.tasks` + transition methods (create/report/cancel/fail/
  finish-record/input_required-on-turn-end/background-ready, each pairing
  mutate + emit per the AGENTS.md rule — NO direct task-state writes from hook
  handlers, the background step, or recovery) in
  `crates/signaltty-server/src/store.rs`; `Snapshot.tasks` serde-defaulted +
  restore in `crates/signaltty-server/src/persist.rs`; Pane lineage fields
  (`parent_pane_id`, `root_pane_id`, `label`, `relationship`, `task_id`)
  serde-defaulted in `crates/signaltty-core/src/model.rs`; `target_branch`
  recorded at start; legacy-snapshot
  load test in `crates/signaltty-server/tests/orchestrator_tasks.rs`.
- [x] T006 [P] `max_parallel_tasks` (default 4) server flag + config in
  `crates/signaltty-server/src/config.rs` and `crates/signaltty-server/src/main.rs`.

**Checkpoint**: Entity + protocol exist; legacy snapshots load; nothing callable yet.

## Phase 3: US2 submit + US3 reads (lane B; pane primitives)

**Independent Test**: submit accept/refuse/stall on synthetic-hook panes;
rendered/incremental reads on TUI fixtures.

- [x] T007 [US2] Failing tests: submit accept on idle/done, `AGENT_BUSY` on
  working/blocked with zero bytes written (read-before/after), `AGENT_NOT_READY`
  on unknown, on never-started panes, and on a worker pane whose task is still
  `pending` (background first-submit in flight), `PANE_EXITED` on exited,
  activity-gate `TIMEOUT`, fast-completion
  match in `crates/signaltty-server/tests/orchestrator_submit.rs`.
- [x] T008 [P] [US3] Failing tests: tail-vs-rendered equality on TUI repaint
  fixtures, incremental delta-only second read, dropped-range signal after
  eviction/restart in `crates/signaltty-term/tests/` (new) +
  `crates/signaltty-server/tests/orchestrator_read.rs`.
- [x] T009 [US2] `pane.submit` handler + typed params in
  `crates/signaltty-server/src/router.rs` /
  `crates/signaltty-server/src/params.rs` (gate → baseline → bracketed paste →
  delay → `\r` → activity gate via existing `WaitBaseline`; accepted submit on
  an `input_required` task's worker pane moves the task to `working` via the
  Store transition method; submit to a `pending` task's worker pane refused
  `AGENT_NOT_READY`).
- [x] T010 [US3] Rendered ring: vt100 scrollback 5000 + `content_seq` + grid
  extraction in `crates/signaltty-term/src/headless.rs`; rebase `tail` on it;
  add `mode: "rendered"` (`after_seq`/`lines` → `text`/`seq`/`next_seq`/
  `dropped`/`truncated`) in `crates/signaltty-server/src/router.rs` +
  `crates/signaltty-server/src/params.rs`.
- [x] T011 [P] [US2] `signaltty pane submit` CLI in `crates/signaltty-cli/src/main.rs`.
- [x] T012 [P] [US3] `pane read --mode rendered --after-seq` CLI in
  `crates/signaltty-cli/src/main.rs`.

**Checkpoint**: US2 + US3 green on panes without any task involved.

## Phase 4: US1 task start (lanes A+C; sync cap → base → worktree → pane, async background ready + submit)

**Independent Test**: async start in temp repo (returns `pending`, background
moves to `working`); assert worktree/branch/base/target_branch/pane/
lineage/env; over-cap refused with nothing created.

- [x] T013 [US1] Failing tests: async happy-path start (returns `pending`, then
  `task.wait --until working` succeeds after background ready + submit),
  `RATE_LIMITED` over cap (nothing created), empty objective refused, bad base
  ref refused, existing branch adopted as pre-existing, concurrent same-path
  starts serialized, default path under `$XDG_DATA_HOME/signaltty/worktrees/`
  (explicit `path` wins), `target_branch` recorded (detached HEAD → unset),
  `task.get`/`task.list` round-trip in
  `crates/signaltty-server/tests/orchestrator_tasks.rs`.
- [x] T014 [US1] `data_dir()` (`XDG_DATA_HOME`, `~/.local/share` fallback — same
  pattern as `state_dir`/`config_dir`) in `crates/signaltty-core/src/paths.rs`;
  base resolution + `target_branch` recording + locked worktree create + branch
  adoption in `crates/signaltty-server/src/git.rs` and
  `crates/signaltty-server/src/worktrees.rs` (per-path lock, defaults for
  branch/path, `fetch_first`).
- [x] T015 [US1] `task.start` handler + typed params in
  `crates/signaltty-server/src/router.rs` /
  `crates/signaltty-server/src/params.rs` (cap → base → worktree → spawn with
  lineage/task env in `crates/signaltty-server/src/pty.rs` synchronously,
  returning `{task, pane}` at `pending`; ready-wait + submit as a server-owned
  background step via Store transitions — survives client disconnect, cancelled
  by cancel/discard/shutdown; failures → failed-with-evidence with `{stage, …}`).
  The `Stop`-with-no-report → `input_required` transition lives in the hook
  handler (lane A, Store transition method); the `Stop`-vs-report race resolves
  by checking report arrival first (report wins).
- [x] T016 [P] [US1] `pane.spawn --parent-pane/--label/--relationship` + `task
  start` CLI in `crates/signaltty-cli/src/main.rs`.
- [x] T017 [P] [US1] `task.get` / `task.list` handlers + CLI in
  `crates/signaltty-server/src/router.rs` and `crates/signaltty-cli/src/main.rs`
  (failing round-trip tests already in T013).

**Checkpoint**: US1 green; E2E legs 1–3 pass.

## Phase 5: US4 results + needs-user (lane A; report/wait/attention)

**Independent Test**: reports store + emit + transition; context wait ends on
all-terminal; permission block → ranked listing → native answer → resume.

- [x] T018 [US4] Failing tests: report stores + emits `task.result` and moves
  state per status; second report refused; unknown task refused; `task.wait`
  single + context + empty-context-immediate + timeout + default-`settled`
  ending at `input_required`; turn-ended-without-report → `input_required`
  with `{reason, last_message?}` → follow-up submit moves back to `working` →
  report completes; `attention.pending`
  rank order with mixed panes; `task.*` glob subscribe + `task_ids[]` filter
  with replay; native-permission block/answer leg (reuse the
  `provider_fixture` pattern from
  `crates/signaltty-server/tests/native_permissions.rs`) in
  `crates/signaltty-server/tests/orchestrator_tasks.rs`.
- [x] T019 [US4] `task.report` + `task.wait` (default `until: settled`; single-flight,
  EOF/shutdown cancel) + `attention.pending` handlers + typed params +
  `task_ids[]` subscribe filter in `crates/signaltty-server/src/router.rs` /
  `crates/signaltty-server/src/params.rs`.
- [x] T020 [P] [US4] `signaltty report`, `task wait`, `attention` CLI in
  `crates/signaltty-cli/src/main.rs`.

**Checkpoint**: US4 green; E2E legs 4–5 pass.

## Phase 6: US5 diff + finish (lane C; review gate)

**Independent Test**: diff-vs-base incl. untracked with clean index; merge
happy/conflict/dirty paths; discard scoping; branch rules; cleanup-error
separation.

- [x] T021 [US5] Failing tests: `task.diff` shows only own change vs base
  (tracked + untracked), index untouched (porcelain clean apart from test
  edits); merge lands + disposition; conflict → `MERGE_CONFLICT`, target
  clean, files named; dirty source/target refusals; staged-work refusal;
  pre-existing branch never deleted; merge into non-recorded checkout refused
  (`BAD_PARAMS` `{expected, actual}`); detached-HEAD start requires explicit
  `target_ref` at finish; cleanup-error separation; discard removes
  only the task path; second finish refused; `task.cancel` keeps checkout in
  `crates/signaltty-server/tests/orchestrator_finish.rs`.
- [x] T022 [US5] `task.diff` / `task.file_diff` (base-SHA baseline, direct
  untracked reads, no `add -N`) in `crates/signaltty-server/src/git.rs` and
  `crates/signaltty-server/src/file_diff.rs` + handlers in
  `crates/signaltty-server/src/router.rs`.
- [x] T023 [US5] `task.finish` (merge/discard rules incl. recorded-target
  checkout check, abort-on-conflict,
  guarded cleanup, disposition recording) + `task.cancel` in
  `crates/signaltty-server/src/router.rs` (+ git helpers in
  `crates/signaltty-server/src/git.rs` /
  `crates/signaltty-server/src/worktrees.rs`).
- [x] T024 [P] [US5] `task diff/file-diff/finish/cancel` CLI in
  `crates/signaltty-cli/src/main.rs`.

**Checkpoint**: US5 green; E2E legs 6 + 8 pass (restart leg still red).

## Phase 7: US6 restart survival (lane A; recovery wiring)

**Independent Test**: restart mid-run → completed intact, running-with-dead-pane
failed with evidence, checkouts preserved; pane exit fails task exactly once.

- [x] T025 [US6] Failing tests: restart with mixed tasks (completed intact +
  results + diffs work; running → failed with evidence + checkout preserved;
  `pending` → failed with `{stage: "restart"}`);
  pane kill → failed exactly once (no double emit on close-after-exit) in
  `crates/signaltty-server/tests/orchestrator_e2e.rs`.
- [x] T026 [US6] Pane-exit hook → task fail in
  `crates/signaltty-server/src/store.rs`; restart recovery in
  `crates/signaltty-server/src/persist.rs` (`apply`); read cursors reset as
  dropped after restart in `crates/signaltty-term/src/headless.rs`.

**Checkpoint**: US6 green; full E2E (incl. `#[ignore]` removal) green.

## Phase 8: US7 GUI rows (lane D; minimal surfacing)

**Independent Test**: worker pane row shows label + state; updates on
`task.updated` without terminal rebuild; light + dark screenshots.

- [x] T027 [US7] Headless GUI tests for the task chip (present/absent/update)
  in `crates/signaltty-gui/src/sidebar.rs` (or `app_tests.rs` if the harness
  fits better — keep display tests out unless no other seam exists).
- [x] T028 [US7] `task.*` subscription + `task.get` cache + row chip render in
  `crates/signaltty-gui/src/actor.rs` and
  `crates/signaltty-gui/src/sidebar.rs`.

**Checkpoint**: US7 green with screenshot evidence.

## Phase 9: Polish & cross-cutting (lane D)

- [ ] T029 [P] Docs rows: Task in `docs/02-data-model.md`, one row per new/
  changed method and event in `docs/08-ipc.md` (`task.start/get/list/wait/
  report/diff/file_diff/finish/cancel`, `pane.submit`, `pane.spawn` lineage
  params, `pane.read` rendered mode, `attention.pending`, `task.*` events —
  re-check every method has both an integration-test task above and a docs/08
  row), plus the self-printing schema extension (specs/002 rule: every schema
  method must dispatch; extend the drift test). Implementers update;
  ADR-0020 already written by the spec author — amend only if implementation
  diverged, with reasons.
- [ ] T030 [P] Agent-facing orchestrator loop doc IN the shipped skill
  (`crates/signaltty-cli/assets/SKILL.md`, served by `signaltty skill` — check
  `skill.rs` embedding): worked example — start 3 tasks back-to-back, `task
  wait` to `settled`, incremental `pane read --mode rendered`, handling
  `input_required` (follow-up submit), `task diff`, `task finish --merge` /
  `--discard` — with the `signaltty schema` pointer for the exact contract.
- [ ] T031 Run `quickstart.md` end-to-end manually, then `scripts/verify.sh
  full`; attach evidence; independent diff review; un-ignore the E2E test.

## Dependencies & execution order

- Phase 1 → Phase 2 → (Phase 3 ∥ Phase 4 supports 5/6/7; Phase 3 is
  pane-only and can run ∥ Phase 4 after Phase 2) → Phase 5 → Phase 6 →
  Phase 7 → Phase 8 → Phase 9.
- Within a story: tests MUST fail before implementation; core before
  handlers; handlers before CLI.
- Shared-file protocol: `router.rs`/`params.rs` single-writer at a time
  (Phase 3 lane B, then Phases 4–7 lane A/C sequential edits). Test files are
  owned per area — implementers never co-edit: `orchestrator_e2e.rs`
  (acceptance scenario + restart), `orchestrator_submit.rs` (US2),
  `orchestrator_read.rs` (US3, plus `signaltty-term` crate tests),
  `orchestrator_tasks.rs` (US1/US4/US6 task-state tests), `orchestrator_finish.rs`
  (US5). Shared fixtures/helpers live in `signaltty-testkit` (T002).
  `headless.rs` lane B only until Phase 7's cursor-reset line (one hunk,
  coordinate).
- Suggested MVP cut: Phases 1–5 (spawn/submit/read/report/wait) already
  orchestrate; 6–7 complete review + survival; 8 is the GUI chip.

## Parallel example

```bash
# After Phase 2: two implementers, disjoint files
# Lane B: T007+T009 (router pane paths) then T008+T010 (headless.rs)
# Lane C: T014 (git.rs/worktrees.rs)
# Lane D: T011+T012+T016 (cli/main.rs) once params are stable
```
