# IPC additions

`pane.get {pane_id}` adds sibling `wait_baseline` to the existing `pane` result.

`wait {pane_id, until: string | string[], after?: WaitBaseline, timeout_s?: u64}`.
Nonempty outcomes must be recognized states. Baseline matching requires a newer
transition on the matching axis and the same process/session. Results retain
satisfied/lifecycle/attention and add outcome/transition_seq. Errors retain TIMEOUT,
NO_SUCH_PANE and BAD_PARAMS, adding IDENTITY_CHANGED. Long waits are single-flight
per connection; disconnect, another input line or shutdown cancels the wait.

`subscribe {events?: string[], from_seq?: u64}` adds replay coverage when requested.
Status is complete, history_lost, truncated, unavailable or cursor_ahead. Complete
replay precedes live delivery and contains no duplicates across the frozen fence.
Incomplete replay returns no event prefix and requires snapshot_then_resubscribe.
Lag or out-of-order live delivery terminates the stream. PTY output is live-only;
reattach snapshots replace the visible screen, with offsets removing overlap.
