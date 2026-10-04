export PATH="$REPO/target/debug:$PATH"
rm -rf /tmp/orch-c && mkdir -p /tmp/orch-c/home && cd /tmp/orch-c
export XDG_DATA_HOME=/tmp/orch-c/data HOME=/tmp/orch-c/home
git init -q -b main . && git config user.name e2e && git config user.email e2e@example.invalid
echo base > f && git add . && git commit -qm base
export SOCK=/tmp/orch-c/sock
signaltty-server --socket "$SOCK" --state-dir /tmp/orch-c/state > server.log 2>&1 &
SERVER=$!; trap 'kill $SERVER 2>/dev/null' EXIT
export SIGNALTTY_SOCKET="$SOCK"
until signaltty status >/dev/null 2>&1; do sleep 0.2; done
signaltty --json task start --repo /tmp/orch-c --context c --label w --agent codex --objective x -- sh > t.json
P=$(jq -r .pane.id t.json); T=$(jq -r .task.id t.json)
signaltty hook-event --agent codex --event SessionStart --pane "$P" >/dev/null; sleep 1
signaltty hook-event --agent codex --event UserPromptSubmit --pane "$P" >/dev/null
signaltty --json task wait "$T" --until working --timeout 20 | jq -c '.tasks[0].state'
PID=$(pgrep -P $SERVER -x sh | head -1); echo "pid=$PID"
[ -n "$PID" ] && kill -9 "$PID"
signaltty --json task wait "$T" --timeout 20 | jq -c .tasks[0]
