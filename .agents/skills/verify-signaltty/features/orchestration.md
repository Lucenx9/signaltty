# Agent orchestration and event recovery

## Sub-features

Current-state waits, new-work baselines, multiple outcomes, identity replacement,
wait cancellation, cursor continuity and honest replay coverage.

## How to get to it (agent POV)

Capture `signaltty --json pane get ID` before submitting work. Save the complete
`wait_baseline` and pass it to `signaltty wait --pane ID --until done,blocked,failed
--after-baseline "$baseline" --timeout 300`. Read outcome/transition_seq separately
from current lifecycle/attention: a brief matching outcome remains observable.
An ordinary wait without a baseline retains immediate current-state matching.

## Driving it with real IPC/CLI

`cargo test -p signaltty-server --test integration baseline_wait` uses real isolated
servers and the CLI binary. It proves old Done rejection, fast/brief outcomes,
first-session discovery, subsequent replacement, malformed params and restart identity.
`attached_pane_streams_after_exit_and_resume_without_reattaching` proves resume changes
process identity while the existing attached stream keeps working.

`new_events_after_restart_advance_the_previous_cursor` and
`subscribe_declares_corrupt_history_and_cursor_ahead` drive replay through real sockets.
Store/audit unit tests force replay cap/filter/retention and removed/truncated/legacy
history, including loss immediately before rotation. Server Unix-pair tests prove
exact replay/live handoff, lag-close and immediate receiver cleanup on EOF/shutdown.

## Declarative progress and Grok detection

`cargo test -p signaltty-server --test screen_detection` runs real PTYs against
isolated servers. Progress fixtures prove working/completion transitions,
`pane.explain` evidence, visible Grok blockers taking priority, semantic-hook
authority, and no attention from ordinary-terminal progress. Exit/resume/restart
fixtures prove the evidence lifetime follows the managed PTY.
`bundled_grok_without_osc_progress_stays_working_until_the_prompt_returns`
uses signaltty terminal identity and no OSC progress. It proves spinnerless
`[stop]` activity cannot finish early, blockers take priority, and completion
raises unread only after the prompt returns.

`cargo test -p signaltty-term osc::tests` covers progress framing, BEL/ST and
split boundaries, malformed messages, and separation from notifications/bells.
`cargo test -p signaltty-agent grok_` covers bundled priorities, title/footer
fallbacks, splash false positives, program scoping and launcher discovery.
These fixtures do not authenticate or execute paid Grok turns. Foreground
program changes within one shell retain progress until overwritten.

## Gotchas

Capture before input and serialize concurrent input writers when turn attribution
matters. Same-state hooks do not prove new work. Process/session replacement returns
IDENTITY_CHANGED; never resubmit mutations after disconnect automatically. Replay
numeric holes are valid. Incomplete replay sends no prefix and closes; open a fresh
subscription before fetching authoritative snapshots. State-event journal writes are
synced and fail closed; snapshot durability remains separate. Uncertain legacy disk
history requires snapshots; the new runtime ring can prove recent intervals.

## Harness sandbox workspace access

`cargo test -p signaltty-server --test mcp_bridge` drives the real CLI's stdio
handshake, discovery, hosting context, worker PTY/output, malformed requests,
missing/stale context, stream exclusions and oversized input. Configuration tests
in signaltty-integration cover four provider roots, foreign entries, disables,
idempotence, atomic writers and owned cleanup.
`python3 scripts/qa-harness-socket.py` uses installed Codex's actual Linux
workspace-write sandbox and an isolated server. It proves direct connect stays
EPERM before/after a host-owned bridge starts a worker, error guidance is usable,
and native Codex accepts the registration. It performs no authenticated model
turn and does not prove every provider's runtime MCP child boundary. See the
support/proof matrix in docs/07-agents.md.

## Verified PR readiness

`cargo test -p signaltty-core workflow` verifies complete merge evidence, draft and
unknown facts, exact five-minute expiry, future timestamps, and legacy snapshots.
`cargo test -p signaltty-server --test orchestrator_pr` uses fake gh and real IPC
to verify head changes, draft/conflict states, malformed observations, failed
refresh invalidation, batch error reporting, and shared task get/list stages.
`cargo test -p signaltty-server --lib pr_refresh_orders_attempts` verifies response
ordering and preservation of the latest successful facts after a failed attempt.
The CLI `cli_task_orchestration` test checks stage in JSON and normal task output.

The ignored GTK test `task_board_shows_columns_and_navigates_to_pane` renders
ready, draft, and failed-refresh PR cards in light and dark, including the compact
opening height and independent wrapped card context/state hierarchy. It verifies expired
readiness moves columns when the board reevaluates without a server event.
These fixtures do not certify live GitHub branch rules or perform a remote merge.

## Accepted task report retries

`cargo test -p signaltty-core task_report` verifies identical content preserves
the accepted result and task metadata, while changed content and unreported
terminal tasks remain refused. `cargo test -p signaltty-server --test
orchestrator_tasks task_report` verifies reconnects, both selectors, optional-field
defaults, concurrent reports, snapshot-restored retries, and one paired result/update
notification. Only identical `task.report` content has this retry guarantee. The
fixtures do not prove synchronous snapshot flushing or coordinator delivery.

## Automatic agent execution

Enable Run agents without confirmations once in Preferences to select native
automatic execution for new installed-command Codex/Claude/Antigravity
workspaces. Codex runs without its native sandbox. Custom commands retain their
arguments. Existing sessions keep their recorded launch mode.

`cargo test -p signaltty-server --test automatic_execution` verifies actual
Codex resume arguments after snapshot restoration, default same-provider native
worker inheritance, explicit worker argv and cross-provider precedence. Pure
argument tests in signaltty-agent cover all three native prefixes and standard
resume shapes; Antigravity workers still require explicit argv.

The isolated ignored GTK tests
`app::tests::automatic_agent_permissions_are_persistent_native_controls` and
`new_workspace::tests::automatic_workspace_launches_preserve_custom_commands`
exercise preference save/load, native light/dark controls and actual form
submission arguments. Full verification runs both; the first emits
`automatic-permissions-light.png` and `automatic-permissions-dark.png`. These
fixtures do not authenticate providers or certify paid turns or remote installations.
