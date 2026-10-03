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

## Gotchas

Capture before input and serialize concurrent input writers when turn attribution
matters. Same-state hooks do not prove new work. Process/session replacement returns
IDENTITY_CHANGED; never resubmit mutations after disconnect automatically. Replay
numeric holes are valid. Incomplete replay sends no prefix and closes; open a fresh
subscription before fetching authoritative snapshots. State-event journal writes are
synced and fail closed; snapshot durability remains separate. Uncertain legacy disk
history requires snapshots; the new runtime ring can prove recent intervals.
