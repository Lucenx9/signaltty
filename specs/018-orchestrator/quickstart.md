# Quickstart: orchestrator

Prereqs: `scripts/setup-agent.sh all`, `scripts/verify.sh doctor`. All commands
below are the deterministic E2E path (fake workers, no LLM); see `spec.md` for
the full acceptance scenario and `contracts/ipc.md` for the exact schemas.

## 1. Start three tasks in a scratch repo

The workers are plain `sh` panes declared as `codex` workers, so the
server maps the hook events below. Nothing here needs an LLM: each "worker"
is driven by hooks, the way a real harness reports them. Run this from the
repo root after `cargo build --workspace`.

```sh
export PATH="$PWD/target/debug:$PATH"
rm -rf /tmp/orch-e2e && mkdir -p /tmp/orch-e2e/home && cd /tmp/orch-e2e
export XDG_DATA_HOME=/tmp/orch-e2e/data HOME=/tmp/orch-e2e/home
git init -q -b main . && git config user.name e2e && git config user.email e2e@example.invalid
echo base > file.txt && git add . && git commit -qm base
export SOCK=/tmp/orch-e2e/sock STATE=/tmp/orch-e2e/state
signaltty-server --socket "$SOCK" --state-dir "$STATE" > server.log 2>&1 &
SERVER=$!; trap 'kill $SERVER 2>/dev/null' EXIT
export SIGNALTTY_SOCKET="$SOCK" CTX="tctx_e2e"
until signaltty status >/dev/null 2>&1; do sleep 0.2; done
for i in 1 2 3; do
  signaltty --json task start --repo /tmp/orch-e2e --context "$CTX" --label "worker-$i" \
    --agent codex --objective "task $i: add file $i" -- sh > "task$i.json"
done
```

Starts are async: each reply carries `{task, pane}` with the task `pending`;
`task.base_sha` and `task.target_branch` are recorded at start. The server
then waits for each worker pane to become `idle`, pastes the composed prompt,
and moves the task to `working` once the worker starts its turn. Fake
workers do that with two hooks, `SessionStart` (ready) and, after the
prompt was pasted, `UserPromptSubmit` (turn started):

```sh
for i in 1 2 3; do
  signaltty hook-event --agent codex --event SessionStart --pane "$(jq -r .pane.id task$i.json)"
done
sleep 1   # the server pastes each prompt as soon as its pane is idle
for i in 1 2 3; do
  signaltty hook-event --agent codex --event UserPromptSubmit --pane "$(jq -r .pane.id task$i.json)"
done
signaltty --json task wait --context "$CTX" --until working --timeout 60 | jq '[.tasks[].state]'
```

Default worktree roots live under `$XDG_DATA_HOME/signaltty/worktrees`
(fallback `~/.local/share`); `git -C /tmp/orch-e2e worktree list` shows them.

## 2. Drive the fake workers (hooks, not keystrokes)

Each worker edits in its own worktree and **commits** (a merge refuses an
uncommitted source tree), then reports:

```sh
T1=$(jq -r .task.id task1.json); T2=$(jq -r .task.id task2.json); T3=$(jq -r .task.id task3.json)
PANE1=$(jq -r .pane.id task1.json); PANE2=$(jq -r .pane.id task2.json); PANE3=$(jq -r .pane.id task3.json)
finish_work() {   # $1 = worker number
  W=$(jq -r .task.worktree_path "task$1.json")
  echo "line $1" > "$W/file$1.txt"
  git -C "$W" add "file$1.txt" && git -C "$W" commit -qm "task $1"
  signaltty --json report --status completed --summary "did task $1" \
    --task "$(jq -r .task.id task$1.json)" > /dev/null
}
finish_work 1
```

Worker 2 blocks first: a `PermissionRequest` with a structured decision shows
up in `attention`; answer it, then the worker finishes:

```sh
signaltty hook-event --agent codex --event PermissionRequest --pane "$PANE2" \
  --decision '{"id":"d_e2e","prompt":"Allow edit?","options":[{"id":"yes","label":"Yes"},{"id":"no","label":"No"}]}'
signaltty --json attention
signaltty decision answer --pane "$PANE2" --decision d_e2e --option yes
finish_work 2
```

Worker 3 ends its turn WITHOUT reporting (synthetic `Stop` hook, no `report`
call): its task moves to `input_required` (`turn_ended_without_report`). Send
a follow-up; the worker starts a new turn (`UserPromptSubmit`) and reports:

```sh
signaltty hook-event --agent codex --event Stop --pane "$PANE3"
signaltty --json task wait "$T3" --timeout 60 | jq '.tasks[0].state'   # default --until settled: "input_required"
signaltty --json pane submit "$PANE3" --text "please report your result now" > submit.json &
SUBMIT=$!
sleep 1   # the worker sees the prompt and starts its turn
signaltty hook-event --agent codex --event UserPromptSubmit --pane "$PANE3"
wait $SUBMIT
finish_work 3
```

## 3. Wait, read, list

```sh
signaltty --json task wait --context "$CTX" --timeout 60 | jq '[.tasks[].state]'   # default --until settled
signaltty --json pane read "$PANE1" --mode rendered --after-seq 0 | jq .next_seq
signaltty --json attention   # empty once all are answered + seen
```

## 4. Diff vs base, restart, finish

```sh
signaltty --json task diff "$T1" | jq '[.files[].path]'   # only worker-1's change
# restart the server on the same socket/state dir, then:
kill $SERVER; wait $SERVER || true
signaltty-server --socket "$SOCK" --state-dir "$STATE" >> server.log 2>&1 &
SERVER=$!
until signaltty status >/dev/null 2>&1; do sleep 0.2; done
signaltty --json task get "$T1" | jq .task.state          # still completed, result intact
signaltty --json task finish "$T1" --merge                # target defaults to the recorded target_branch
signaltty --json task finish "$T2" --merge --delete-branch
signaltty --json task finish "$T3" --discard
# Merge refuses if the source repo no longer has the recorded target checked out:
#   BAD_PARAMS {expected: "main", actual: "other"} — switch back first.
git -C /tmp/orch-e2e log --oneline       # two merge commits; main untouched otherwise
git -C /tmp/orch-e2e worktree list       # no task worktrees left
```

## 5. Failure paths (each its own test)

- Dirty target: edit `file.txt` in `/tmp/orch-e2e` uncommitted → `task finish
  --merge` → `BAD_PARAMS` (dirt), nothing merged.
- Conflict: two workers edit the same line → second merge → `MERGE_CONFLICT`,
  target `git status --porcelain` clean, conflicted files named.
- Crash: `kill` a worker pane's child → task `failed` with evidence.
- Cap: start the server with `signaltty-server --max-tasks 1`, start two → second is `RATE_LIMITED`, creates nothing.
- Stall: submit to an idle shell that never emits hooks → `TIMEOUT`
  (`details.stage: "activity_gate"`) after ~5 s.

## 6. Verify

`scripts/verify.sh full`; inspect `target/verification/` evidence. GUI row
surfacing: light + dark screenshots per the constitution gate.

## Pointers

- Live contract: `signaltty schema` (same constants the router dispatches
  on); IPC tables in `docs/08-ipc.md`, Task entity in `docs/02-data-model.md`.
- Agent loop: "Orchestrating other agents" in
  `crates/signaltty-cli/assets/SKILL.md` (served by `signaltty skill`).
