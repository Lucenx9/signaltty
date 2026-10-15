# Research and design synthesis

## Grounded behavior and rationale

The review at fd992ef found four independently implemented command paths. ADR-0016 places preview limits, Git conversion compatibility and cleanup in selected-file review. ADR-0029 captures trees through a copied private index. ADR-0020 requires bounded merge execution and recovery. Current source and real-process tests are authoritative for operation-specific command options, deadlines and errors.

A new caller-level test canceled snapshot capture while a Git clean filter ran. It failed on the baseline with `a canceled snapshot left its Git filter running`, then passed after the turn caller used the shared lifetime module. This makes synchronous process-group termination on caller drop a demonstrated requirement.

## Alternatives

1. Direct owned future. The module privately owns Child, drains both pipes concurrently, then waits. Drop terminates the group synchronously while the leader is unreaped; Tokio arranges eventual reaping. One execution call hides lifetime rules.
2. Detached supervisor. A task owns execution and receives cancellation over a channel. It can await reaping after caller drop, but cancellation becomes asynchronous. Making termination synchronous needs shared group state and a protocol to prevent racing the supervisor's reap.
3. Shared process-group guard alone. Rejected because every caller still coordinates spawn, reads, wait, disarm and error cleanup.

## Candidate comparison

The parent and an independent GPT-5.6 design converged on direct ownership and drain-before-wait. The independent candidate proposed richer RunOptions/CaptureLimit/Stream types and separate read/wait errors. The implementation retains only distinctions current caller mappings need: capture limits, optional deadline, spawn/read/timeout and stream overflow. Additional builders and stage metadata were rejected because callers do not use them.

A Claude candidate could not run because its provider reported a rate limit. A Grok candidate produced no artifact during the bounded design window and was canceled. Neither is counted as review evidence. Independent Standards and Spec reviews found no actionable findings. The design cross-judge preferred the smaller parent interface with the independent candidate's complete policy inventory and test requirements. Its cancellation-test finding was accepted: the exited-leader case now waits for the owned leader to reach zombie state before aborting. The no-signal-after-reap guarantee is also checked structurally against Tokio Child::id behavior.

## Synthesis decision

Use direct ownership as the base. Keep the leader unreaped while pipes remain open, rather than joining wait with reads and retaining an armed numeric process-group guard after leader exit. Use Child::id as the ownership indicator so no copied identifier survives successful wait. Preserve existing limits and deadlines at the callers, including configured worktree fetch and merge deadlines rather than replacing them with the default twenty seconds. Retain operation-level tests and add real executable tests through the shared interface, including descendants retaining pipes after leader exit.

## Tradeoffs

Cancellation kills the group synchronously but direct-child reaping after future drop remains Tokio's best-effort runtime cleanup, verified during a live runtime. An exited leader may remain a zombie until pipes reach EOF or the operation ends. These costs avoid detached task/channel coordination and a stale group identifier. Escaped process groups remain outside the established contract.
