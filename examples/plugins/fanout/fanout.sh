#!/bin/sh
# Pane launcher: one pane per command, all in a fresh workspace.
# Usage: signaltty plugin run fanout spawn -- 'codex exec "..."' 'claude -p "..."'
# Requires: signaltty on PATH, python3 for JSON parsing.
set -eu
if [ "$#" -eq 0 ]; then
  echo "usage: signaltty plugin run fanout spawn -- '<cmd>...'" >&2
  exit 2
fi
CLI="${SIGNALTTY_CLI:-signaltty}"
FIRST="$1"; shift
IDS=$(SIGNALTTY_SOCKET="$SIGNALTTY_SOCKET" "$CLI" new --cwd "$HOME" --json -- sh -c "$FIRST")
WS=$(echo "$IDS" | python3 -c 'import json,sys; print(json.load(sys.stdin)["workspace_id"])')
PANE=$(echo "$IDS" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pane_id"])')
echo "workspace $WS"
echo "pane $PANE <- $FIRST"
for cmd in "$@"; do
  PANE=$(SIGNALTTY_SOCKET="$SIGNALTTY_SOCKET" "$CLI" pane split "$PANE" --json -- sh -c "$cmd" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["pane"]["id"])')
  echo "pane $PANE <- $cmd"
done
