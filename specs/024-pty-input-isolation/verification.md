# Verification and handoff

Branch/worktree: `t3/orchestration-pty-backpressure`,
`/home/simone/.t3/worktrees/signaltty/t3-6007730f`.
Base: `4815c16678dad0128053bde775a06b9691592da5` (includes merged PR 38).

## Red → green

The real server owns two isolated PTYs. A Python child enters raw mode and
waits for an external file before consuming input. Sending 256 KiB saturates
its input buffer; no paid agent or default socket is involved.

Command (pinned Rust 1.94.0):

```sh
PATH=/home/simone/.cargo/bin:$PATH cargo test -p signaltty-server --test pty_input_backpressure -- --nocapture
```

On the unchanged implementation, independent input and TERM signal lookup both
timed out. The first red run failed in 2.49 seconds with
`a stalled pane must not block another pane's input: Elapsed(())`.
Adding a store-only status probe also timed out (3.49 seconds); moving only the
writer lock off the registry remained red. Moving raw input to the blocking
pool made those operations responsive with one Tokio executor thread.

A TERM probe then exposed a separate limit: the large kernel write could remain
pending even after child exit. A debugger stack showed the blocking-pool thread
in `write → portable_pty::UnixMasterWriter → PtyManager::write_input`.
The final fixture uses USR1 with a child-side acknowledgment before releasing
the reader, so it verifies actual signal delivery and always drains its input.
It also queues a second same-pane input and records exact bytes to verify
non-interleaving and both successful byte counts. All assertions happen after
fixture release and owned-server teardown.

Final focused regression: passed, 0.59 seconds. All 13 existing orchestrator
submit regressions passed, including permission-during-paste and recoverable
background submission.

## Shared verification

`PATH=/home/simone/.cargo/bin:$PATH scripts/verify.sh full` passed in
`target/verification/full-26z4cr8k/summary.json`: tooling, formatting, architecture,
Clippy/warning policy, workspace build/tests, server QA, all 24 isolated GTK
tests and refresh benchmark. The same host's formerly failing GTK tests passed
on the current main baseline, which also includes PR 39. Do not attribute that
change to this server patch.

The full run covered the production patch and original new regression. The
stronger same-pane exact-byte assertions were added afterward and passed in the
focused command above. `cargo fmt --all` and `git diff --check` passed afterward.
Native palette light/dark renders were inspected: selected row, text and focus
remain readable. No visual changes are part of this patch. Desktop AT-SPI,
IME and paid provider sessions were not run.

## Review and remaining work

Grok 4.7 (native xAI provider, no OpenRouter) is independently auditing the
backpressure/lock boundary; its result remains pending.

Sonnet 5.5 high completed its independent diff review: **approve, no blockers**.
It ran the regression and decision/submit filters, and independently compared
close/shutdown probes with the old implementation. Its requested documentation
of pending input/thread/fd ownership after close/death and delayed shutdown is
added to ADR-0022. The shutdown cause remains a hypothesis. Follow-ups include
async same-pane serialization before blocking-pool handoff, the Enter waiting
on another writer, writer destruction, and backpressure tests for the paste and
legacy reply paths. Existing normal submit/decision tests pass; those stress
paths and respawn identity are review-supported, not independently stress-tested.

Remaining audit items: make automatic Enter bounded while keeping the decision
check atomic; diagnose/contain a large write that outlives a killed child;
consider finite blocking-pool capacity under many stalled requests. These are
explicit limits, not covered by this isolation regression.

Published draft: https://github.com/Lucenx9/signaltty/pull/42 (linked to this thread).
Next step: consume Grok's result and address any blockers before marking ready.
Check host CI on the exact published head.

## Integration with current main

Rebased onto e6d1845 after PR 40 added resume-after-delivery semantics. The
conflicting helper now awaits an async delivery future: gate consumption still
happens before bytes, resume happens only after successful I/O, and failure
keeps the pane blocked. Existing stale/success/failure tests use that same async
helper. A pending-delivery test asserts Blocked until the delivery future resolves.
Grok approved f9ce543 before this integration; a new pinned review and full
verification are required for the new head. No previous verdict is represented
as approval of the conflict resolution.
