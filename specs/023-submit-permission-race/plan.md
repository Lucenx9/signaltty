# Plan: Prompt submission preserves permission decisions

Rust 2021, existing server Store and PtyManager. No dependencies or architecture
changes. Use the current orchestrator_submit integration seam before changing
submit.rs. Record the red input trace, then recheck pending decisions and
permission attention immediately before delayed Enter. Keep the read guard
through the single-byte Enter writes, including retry, to serialize hook state
changes with Enter. Keep the bulk paste outside the store lock so a full PTY
buffer cannot prevent the output pump from updating the store. Preserve the
existing gate and plain input follow-ups.

Reconcile decisions in Store task resume and background-ready transitions.
A follow-up resume with a pending decision does nothing; a first background
commit parks at input_required with the current decision ID. Unit regressions
replay the real set_decision/submit-commit order before changing the methods.

A delayed-enter AGENT_BUSY with paste_delivered uses the existing unconfirmed
submit recovery. Reconcile any pending decision in that Store transition too;
other startup refusals still fail. Cover this through real task.start IPC.

Constitution: specified before implementation; real-PTY red-green regression;
typed existing IPC boundary; full verification before PR. No ADR required.

Verification: targeted regression, complete orchestrator_submit suite, shared
scripts/verify.sh full. Update docs/08-ipc.md with partial-delivery behavior.
