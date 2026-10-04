# Quickstart: orchestrator

Prereqs: `scripts/setup-agent.sh all`, `scripts/verify.sh doctor`. All commands
below are the deterministic E2E path (fake workers, no LLM); see `spec.md` for
the full acceptance scenario and `contracts/ipc.md` for the exact schemas.

## 1. Start three tasks in a scratch repo

```sh
rm -rf /tmp/orch-e2e && mkdir -p /tmp/orch-e2e && cd /tmp/orch-e2e
git init -q -b main . && echo base > file.txt && git add . && git commit -qm base
export SOCK=/tmp/orch-e2e/sock STATE=/tmp/orch-e2e/state
signaltty-server --socket "$SOCK" --state-dir "$STATE" &
export SIGNALTTY_SOCKET="$SOCK" CTX="tctx_e2e"
for i in 1 2 3; do
  signaltty --json task start --repo /tmp/orch-e2e --context "$CTX" \
    --label "worker-$i" --objective "task $i: append line $i" -- sh > "task$i.json"
done
# Starts are async: each returns at `pending`. Issue all three back-to-back,
# then wait for the background ready + submit on each:
signaltty --json task wait --context "$CTX" --until working --timeout 120
```

Each reply carries `{task, pane}` with the task `pending`; `task.base_sha` and
`task.target_branch` are recorded at start. Default worktree roots live under
`$XDG_DATA_HOME/signaltty/worktrees` (fallback `~/.local/share`) —
`git -C /tmp/orch-e2e worktree list` still shows them all.

## 2. Drive the fake workers (hooks, not keystrokes)

```sh
PANE1=$(jq -r .pane.id task1.json)
signaltty hook-event --agent generic --event SessionStart --pane "$PANE1"
# … worker does its edit, then:
signaltty --json report --status completed --summary "did task 1" --task "$(jq -r .task.id task1.json)"
```

Worker 2 blocks instead: send a `PermissionRequest` with the native waiting
fixture (see `crates/signaltty-server/tests/native_permissions.rs`
`provider_fixture`), confirm it appears in `signaltty --json attention`, answer
via `signaltty decision answer --pane "$PANE2" --decision "$DID" --option "$OID"`
(ids from `attention` / the decision payload), then report. Worker 3 ends its turn WITHOUT
reporting (synthetic `Stop` hook, no `report` call): its task moves to
`input_required` (`turn_ended_without_report`). Send a follow-up and let it
report afterwards:

```sh
T3=$(jq -r .task.id task3.json)
PANE3=$(jq -r .pane.id task3.json)
signaltty --json task wait "$T3" --timeout 60          # default --until settled ends here
signaltty --json task get "$T3" | jq .task.state       # "input_required"
signaltty --json pane submit "$PANE3" --text "please report your result now"
# … worker reports:
signaltty --json report --status completed --summary "did task 3" --task "$T3"
```

## 3. Wait, read, list

```sh
signaltty --json task wait --context "$CTX" --timeout 60   # default --until settled
signaltty --json pane read "$PANE1" --mode rendered --after-seq 0 | jq .next_seq
signaltty --json attention   # empty once all are answered + seen
```

## 4. Diff vs base, restart, finish

```sh
T1=$(jq -r .task.id task1.json)
signaltty --json task diff "$T1"          # only worker-1's change
# restart the server on the same socket/state dir, then:
signaltty --json task get "$T1"           # still completed, result intact
signaltty --json task finish "$T1" --merge          # target defaults to the recorded target_branch
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
