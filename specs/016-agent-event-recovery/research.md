# Research and design synthesis

Source review: [cmux/Herdr code](../../docs/research/2026-10-03-cmux-herdr-code.md).
Grounding traced Store → audit/ring → broadcast → connection, PTY attach offsets,
GUI reconnect and native permission wait cancellation. It also found stale attach
sizes during reconnection, which must be avoided when using that recovery path.

Architect candidates were Codex (runtime progress and a concentrated history owner)
and Grok (persisted Pane counters with audit-owned reservation). Claude did not
produce an artifact after repeated wrap-up requests and was cancelled. The two
completed designs differ in ownership/persistence and both considered boot-scoped
cursors and an atomic prompt-and-wait API as whole-shape alternatives.

| Criterion (1–5) | Codex | Grok |
|---|---:|---:|
| Durable identifier uniqueness and failures | 5 | 4 |
| Honest replay and delayed broadcast fence | 5 | 3 |
| Baseline identity and old wait compatibility | 4 | 3 |
| Small interfaces and ownership | 3 | 4 |
| Behavioral verification | 4 | 2 |

Root and the independent Grok cross-judge agreed on Codex as the base. Graft the
Grok proposal to keep reservations/retention inside the existing audit rather than
adding EventHistory, and publish under the assigning Store lock. Retain runtime
process identities and separate lifecycle/attention stamps. Reject persisted epochs:
restoring a pane would preserve a token for a dead process. Reject successful partial
replay and rename-only sequence reservations. Screened against shallow modules,
pass-through APIs, information leakage and temporal decomposition; no new pipeline
layers are needed. Callers learn only a baseline and replay coverage object.

Scalar reservations preserve existing event/cursor types; boot-scoped cursors expose
cross-boot ordering throughout clients. An atomic prompt-and-wait limits submission
paths and duplicates pane.input, while a baseline works with independent writers.
Callers still serialize concurrent submissions when exact turn attribution matters.

Implementation refinement: audit state records are synced before publication and
rotation retention is persisted before discarding history. Startup or runtime journal
allocation/write failures fail closed; continuing after an unrecorded state event
would make coverage unknowable. PTY sequences need only a reserved range, without
per-byte journal writes. Snapshot persistence remains separate and unchanged.

Verification starts with three demonstrated failing regressions: post-restart
sequence reuse (1 after 5), silent forward output gap, and missing wait baseline.
Full gate and final review results are recorded in verification.md before publication.

Independent review corrected complete-line truncation, runtime generation removal,
legacy uncertainty and brief-outcome waits. Retention stores expected generation
lengths and persistent uncertainty, checked before rotation can overwrite evidence.
Runtime progress remembers the last matching transition per outcome (bounded by the
recognized state vocabulary); responses distinguish that match from current state.
Pane publication/exit/removal explicitly own progress, rather than inferring domain
transitions from JSON payloads in emit.
