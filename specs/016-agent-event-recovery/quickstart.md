# Quickstart

Capture `signaltty --json pane get "$pane"` and save `wait_baseline` before
submitting work. Supply that JSON to `--after-baseline` and request Done,
Blocked or Failed. A completion newer than the captured baseline works even if
it occurs before wait arrival. A current-state wait without a baseline is unchanged.

Event clients inspect subscribe replay metadata. For incomplete coverage, reopen a
subscription without a cursor before refreshing authoritative workspace state and
reattaching terminals. Do not resend input or mutations after connection failure.

Run `scripts/setup-agent.sh all`, `scripts/verify.sh doctor`, then development tests
and `scripts/verify.sh full`. Evidence belongs to target/verification; inspect renders.
