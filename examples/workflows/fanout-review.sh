#!/bin/sh
# Structured agent-to-agent orchestration with server primitives only:
# fan out one pane per prompt, wait for each to finish, collect outputs,
# notify once. No PTY scraping: wait/read/notify are semantic calls.
# Usage: ./fanout-review.sh 'prompt 1' 'prompt 2' ...
# Requires: signaltty on PATH, python3, an agent CLI (example uses codex).
set -eu
CLI="${SIGNALTTY_CLI:-signaltty}"
SOCK="${SIGNALTTY_SOCKET:-$XDG_RUNTIME_DIR/signaltty/signaltty.sock}"
if [ "$#" -eq 0 ]; then echo "usage: $0 '<prompt>...'" >&2; exit 2; fi

PANES=""
FIRST=1
for prompt in "$@"; do
  if [ "$FIRST" -eq 1 ]; then
    OUT=$("$CLI" --socket "$SOCK" new --json -- codex exec "$prompt")
    WS=$(echo "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["workspace_id"])')
    P=$(echo "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pane_id"])')
    FIRST=0
  else
    P=$("$CLI" --socket "$SOCK" pane split "$P" --json -- codex exec "$prompt" \
      | python3 -c 'import json,sys; print(json.load(sys.stdin)["pane"]["id"])')
  fi
  PANES="$PANES $P"
  echo "worker $P <- $prompt"
done

for p in $PANES; do
  "$CLI" --socket "$SOCK" wait --pane "$p" --until done --timeout 1800 >/dev/null
  echo "== $p done"
  "$CLI" --socket "$SOCK" pane read "$p" | tail -20
done
"$CLI" --socket "$SOCK" notify --pane "$P" --title "fanout complete" \
  --body "all workers done in $WS"
