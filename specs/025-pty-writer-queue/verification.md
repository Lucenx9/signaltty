# Verification and handoff

Branch: `t3/pty-writer-queue`; base `f9ce543` (PR 42).

## Red and green

```sh
PATH=/home/simone/.cargo/bin:$PATH cargo test -p signaltty-server --lib same_pane_waiters_leave_blocking_pool_available -- --nocapture
```

The real raw child waits on a test-owned release file. A 256 KiB input holds its
writer; explicitly polling the next same-pane input schedules another blocking
thread on the old code. With a two-thread pool, independent pane input timed out:
`same-pane waiters must not occupy every pool thread: Elapsed(())`, red in 1.03s.
The test releases/drains before asserting and terminates only its owned panes.

After moving the per-writer wait before the blocking handoff, the test passed in
0.03s. The cancellation variant aborts the first awaiting future while its real
I/O remains blocked. The second request stays gated until the closure finishes,
and independent input still progresses. Both tests pass together in 0.03s.

`scripts/verify.sh full` passed: `target/verification/full-xecd79_o/summary.json`,
36 steps including workspace tests, Clippy, server QA, 24 isolated GTK tests and
benchmark. The existing real-IPC exact-byte test verifies non-interleaving.
No GUI code changed; desktop accessibility and paid provider sessions not run.

## Review and next work

Gemini 3.8 Flash high via Antigravity returned `Internal error encountered.`
without a review. The fallback Sonnet 5.5 high review returned an API rate-limit
error without findings. Do not count either as approval. Native xAI Grok 4.7
final review is pending; no OpenRouter used.

This fixes the specific queue pressure finding from Sonnet's approved PR 42
review. Active kernel writes and synchronous Enter remain the next specified
changes; queued cross-connection order is not a wire contract. The owned guard
is transferred into the blocking closure to preserve cancellation ownership.

Spec 026 is drafted in this checkout but is not part of this PR. Next: publish
the queue PR against the isolation branch, consume Grok findings, then merge
both after exact-head CI and continue bounded-input implementation.
