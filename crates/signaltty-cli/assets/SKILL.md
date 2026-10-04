<!-- signaltty-skill -->
# signaltty Agent Skill

You are running **inside a signaltty-managed pane**. Your pane id is in
`$SIGNALTTY_PANE` and the server socket in `$SIGNALTTY_SOCKET`
(defaults apply when unset). This file is the whole contract — read it
once, then ask the server for the rest.

## Gate (read first)

- Act through this API **only when `SIGNALTTY_PANE` is set**. Without
  it you are in a bare shell: plain terminal work still works, but
  `notify`, `hook-event`, `wait` and `decision.answer` will misattribute
  or fail — say so instead of guessing a pane id.
- Never invent pane ids, decision ids, or option ids. Use exactly what
  the server returned (`pane.get`, `decision.created`).

## Learn the contract at runtime

```sh
signaltty schema          # methods, events, error codes, optional capabilities
signaltty pane get "$SIGNALTTY_PANE"
```

`--json` on every command prints the raw `result` object. Unknown JSON
fields are ignored by the server; mistyped ones are `BAD_PARAMS`.

## Things you can do

- `signaltty pane read ID [--mode screen|tail]` — read scrollback.
- `signaltty pane input ID (--data "…" | --stdin)` — type into a pane.
- `signaltty notify --pane ID --title T [--body B] [--severity info|warning|error]`
- `signaltty wait --pane ID --until blocked|done|idle|failed|exited|seen --timeout 300`
  — matches current state immediately when already satisfied. Repeat `--until` or
  use `--until done,blocked,failed` for alternative outcomes. A primary agent fans out with `pane.spawn`,
  `wait`s, then collects with `pane read`. That is the orchestration loop.
- `signaltty decision answer --pane ID --decision DID --option OID` —
  answer a structured approval (`pending_decision` in `pane get`).
- `signaltty hook-event --agent <kind> --event <Name> --payload-stdin` —
  report lifecycle (shims do this; read the hook JSON from stdin).
- `signaltty report-session --pane ID --session SID` — pin a native
  session id when hooks cannot (resumed sessions often fire no start hook).

## Rules

- Attention clears only on explicit per-pane interaction (`pane.mark_seen`,
  focusing the pane, answering). Do not mark other panes seen.
- Prose is not data: a `pending_decision` with options is answerable;
  anything else gets answered inside the terminal, never via a guessed
  `decision.answer`.
- Prefer `wait` + `pane read` over attaching to interactive streams.
- Keep notifications short: title ≤ a few words, body ≤ a line.

## Example: fan out, wait, collect

```sh
A=$(signaltty --json pane spawn --workspace "$WS" --agent codex -- codex exec "write tests" | jq -r .pane.id)
B=$(signaltty --json pane spawn --workspace "$WS" --agent claude -- claude -p "review auth" | jq -r .pane.id)
signaltty wait --pane "$A" --until done --timeout 1800
signaltty pane read "$A" --mode tail --lines 50
```

## Orchestrating other agents

When you are the orchestrator in a pane (work that splits into independent,
mergeable pieces, each in its own worktree), spawn worker tasks instead of
doing it all yourself. Do one thing yourself when the change is small,
needs your checkout, or the pieces cannot be reviewed separately.

The loop — exact commands (`--json` + `jq` to capture ids):

```sh
# 1. Start N tasks back-to-back (each returns at `pending`; prompt is submitted for you).
A=$(signaltty --json task start --repo "$REPO" --context "$CTX" --label "api" \
  --agent claude --objective "Add the endpoint. Acceptance: cargo test passes." | jq -r .task.id)
PA=$(signaltty --json task get "$A" | jq -r .task.pane_id)   # worker pane when needed
# 2. Wait until all settle (default: terminal OR input_required).
signaltty --json task wait --context "$CTX" --timeout 1800
# 3. A task at input_required ended its turn without reporting (or went silent,
#    or needs a permission): inspect, then follow up or answer.
signaltty --json task get "$A" | jq '{state, status_reason}'
signaltty --json pane read "$PA" --mode rendered --after-seq "$SEQ"  # incremental; feed next_seq back
signaltty --json attention                          # who needs the user, ranked
signaltty --json pane submit "$PA" --text "please report your result now"  # back to working
signaltty decision answer --pane "$PA" --decision DID --option OID  # permissions only
# 4. Review each diff against its own base, then finish explicitly.
signaltty --json task diff "$A"
signaltty --json task finish "$A" --merge                  # or --discard; --delete-branch only for task branches
```

Rules: cap is 4 parallel tasks (over-cap → `RATE_LIMITED`, nothing
created). Never auto-merge: read `task diff` first. Workers report via
`signaltty report --status completed|failed|rejected --summary …` (it reads
`$SIGNALTTY_TASK` inside the worker pane). `task.wait` on a context ends at
the first `input_required` too — loop until everything is terminal.
`signaltty schema` is the exact contract when in doubt.

Workers must be agents whose hooks report readiness (`--agent claude`,
`codex`, …); a plain shell never gets ready and fails with `ready_timeout`.
A repo the agent has never trusted stops the worker on its folder-trust
prompt: the failed task's `status_reason.screen_tail` shows that screen.
Trust the repo once in the agent, then retry. A prompt typed but never
confirmed by activity parks at `input_required` with
`reason: submit_unconfirmed`; a nudge, or the worker starting late, resumes it.

Worked example (3 workers, one needs a nudge):

```sh
CTX=tctx_demo; REPO=$PWD
for i in 1 2 3; do
  signaltty --json task start --repo "$REPO" --context "$CTX" --label "worker-$i" \
    --agent claude --objective "task $i: <objective>. Acceptance: <criteria>." > "task$i.json"
done
signaltty --json task wait --context "$CTX" --until working --timeout 300   # background ready + submit
# ... workers work; one ends its turn silently ...
signaltty --json task wait --context "$CTX" --timeout 3600                     # ends at input_required
T3=$(jq -r .task.id task3.json); P3=$(jq -r .pane.id task3.json)
signaltty --json pane submit "$P3" --text "please report your result now"
signaltty --json task wait --context "$CTX" --timeout 3600                     # all terminal
for f in task1 task2 task3; do signaltty --json task diff "$(jq -r .task.id $f.json)"; done
signaltty --json task finish "$(jq -r .task.id task1.json)" --merge
signaltty --json task finish "$(jq -r .task.id task2.json)" --merge --delete-branch
signaltty --json task finish "$(jq -r .task.id task3.json)" --discard
```

## Reuse a pane for new work

Capture the server's baseline before input, then wait for a newer matching transition:

```sh
baseline=$(signaltty --json pane get "$A" | jq -c .wait_baseline)
signaltty pane input "$A" --data "$prompt"
signaltty --json wait --pane "$A" --until done,blocked,failed \
  --after-baseline "$baseline" --timeout 1800
```

An earlier Done cannot satisfy this wait, and fast completion before wait arrival
still succeeds. Repeated same-state hooks do not prove new work. `IDENTITY_CHANGED`
means the process/session was replaced; inspect the pane and capture a new baseline
for future work. Never resubmit input automatically after a disconnected request.
Serialize concurrent submissions to one pane when you need exact turn attribution.

Event clients inspect `subscribe.replay.status`: incomplete history returns no replay
and closes the stream. Subscribe afresh before reading authoritative workspace state
and attaching panes. EOF/lag/byte gaps require that recovery; snapshots restore the
current screen. Prefer the bounded `wait` API over maintaining terminal streams.
