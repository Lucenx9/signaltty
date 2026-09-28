# 03 — Lifecycle & Attention Models

Lifecycle (what the agent is doing) and attention (whether the human
must look) are **independent axes**. Both are first-class in the data
model, IPC, and UI.

## Agent lifecycle

```text
unknown → working ⇄ blocked → done
   │          │         │        │
   │          ▼         ▼        ▼
   │        failed    failed   (terminal)
   ▼
 idle ⇄ working
   │
   └── exited (process gone; orthogonal to task outcome)
```

| State | Meaning | Typical sources |
|---|---|---|
| `unknown` | no adapter signal yet | new pane, generic shell |
| `working` | agent is making progress | hooks (PreToolUse), process running, spinner/title |
| `blocked` | waiting on human (input/permission) | hooks (PermissionRequest), OSC notify, idle+prompt heuristics |
| `done` | turn/task completed | hooks (Stop), `notify` hook, exit 0 of one-shot |
| `idle` | alive, nothing running | hooks (SessionStart before prompt), prompt-readline state |
| `failed` | errored | non-zero exit, `error` hook payload |
| `exited` | PTY child gone | `waitpid`/pipe EOF (keeps last lifecycle as `last_lifecycle`) |

Transitions are adapter-driven; explicit events always override
heuristics. Every transition emits `agent.<state>` with the previous
value and the source.

## Attention

| State | Meaning | UI treatment |
|---|---|---|
| `none` | nothing to see | neutral |
| `unread` | output/activity since last seen | subtle dot |
| `input_required` | agent asked a question | attention ring |
| `permission_required` | approval gate open | strong ring |
| `warning` | non-fatal problem | amber indicator |
| `error` | failure needs triage | red indicator |

Severity order: `error > permission_required > input_required >
warning > unread > none`. A pane shows its highest outstanding item.

## Canonical examples

| Situation | lifecycle | attention |
|---|---|---|
| agent mid-turn | `working` | `none` (subtle activity only) |
| agent asks question | `blocked` | `input_required` |
| approval dialog open | `blocked` | `permission_required` |
| turn finished, not yet seen | `done` | `unread` |
| turn finished, user reviewed | `done` | `none` |
| tool error, agent retrying | `working` | `warning` |
| agent crashed | `failed` | `error` |
| plain shell, new output | `idle` | `unread` |

## Clearing rules

- Attention clears only on **explicit user interaction with that
  pane**: focusing it, marking read, or replying. Visibility of the
  workspace/tab is not enough.
- `mark_seen(pane)` sets `attention=none`, `read_at=now`, keeps
  `lifecycle` untouched, emits `attention.cleared`.
- New signals re-raise attention (`attention.created`).
- Desktop notifications are suppressed when the target pane is already
  focused; clicking one focuses workspace+tab+pane and clears.
- "Jump to next unread" walks panes ordered by severity then recency.

## Sidebar summary (per workspace)

Each sidebar row derives from its panes, never from telemetry:

- name, repo/branch, agent kinds present
- worst lifecycle (blocked > failed > working > done > idle)
- worst attention (severity order above)
- latest explicit message (notification/hook summary, not scraped text)
- time since last attention change
