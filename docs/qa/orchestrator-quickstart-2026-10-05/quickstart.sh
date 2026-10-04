set -x
export PATH="$REPO/target/debug:$PATH"
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
jq -c '{state:.task.state,base:.task.base_sha,target:.task.target_branch}' task1.json
for i in 1 2 3; do
  signaltty hook-event --agent codex --event SessionStart --pane "$(jq -r .pane.id task$i.json)"
done
sleep 1
for i in 1 2 3; do
  signaltty hook-event --agent codex --event UserPromptSubmit --pane "$(jq -r .pane.id task$i.json)"
done
signaltty --json task wait --context "$CTX" --until working --timeout 60 | jq -c '[.tasks[].state]'
git -C /tmp/orch-e2e worktree list
T1=$(jq -r .task.id task1.json); T2=$(jq -r .task.id task2.json); T3=$(jq -r .task.id task3.json)
PANE1=$(jq -r .pane.id task1.json); PANE2=$(jq -r .pane.id task2.json); PANE3=$(jq -r .pane.id task3.json)
finish_work() {
  W=$(jq -r .task.worktree_path "task$1.json")
  echo "line $1" > "$W/file$1.txt"
  git -C "$W" add "file$1.txt" && git -C "$W" commit -qm "task $1"
  signaltty --json report --status completed --summary "did task $1" \
    --task "$(jq -r .task.id task$1.json)" > /dev/null
}
finish_work 1
signaltty hook-event --agent codex --event PermissionRequest --pane "$PANE2" \
  --decision '{"id":"d_e2e","prompt":"Allow edit?","options":[{"id":"yes","label":"Yes"},{"id":"no","label":"No"}]}'
signaltty --json attention | jq -c .
signaltty decision answer --pane "$PANE2" --decision d_e2e --option yes
finish_work 2
signaltty hook-event --agent codex --event Stop --pane "$PANE3"
signaltty --json task wait "$T3" --timeout 60 | jq -c '.tasks[0].state'
signaltty --json pane submit "$PANE3" --text "please report your result now" > submit.json &
SUBMIT=$!
sleep 1
signaltty hook-event --agent codex --event UserPromptSubmit --pane "$PANE3"
wait $SUBMIT; echo "submit exit $?"; cat submit.json | jq -c .
finish_work 3
signaltty --json task wait --context "$CTX" --timeout 60 | jq -c '[.tasks[].state]'
signaltty --json pane read "$PANE1" --mode rendered --after-seq 0 | jq .next_seq
signaltty --json attention | jq -c .
signaltty --json task diff "$T1" | jq -c '[.files[].path]'
kill $SERVER; wait $SERVER || true
signaltty-server --socket "$SOCK" --state-dir "$STATE" >> server.log 2>&1 &
SERVER=$!
until signaltty status >/dev/null 2>&1; do sleep 0.2; done
signaltty --json task get "$T1" | jq -c '{state:.task.state,result:.task.result}'
signaltty --json task finish "$T1" --merge --keep-branch | jq -c .
signaltty --json task finish "$T2" --merge | jq -c .
signaltty --json task finish "$T3" --discard | jq -c .
git -C /tmp/orch-e2e log --oneline
git -C /tmp/orch-e2e worktree list
git -C /tmp/orch-e2e branch
