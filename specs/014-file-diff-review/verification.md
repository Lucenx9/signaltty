# Verification: per-file diff review

All three user stories and FR-001–FR-009 are implemented. The accepted first
increment is current worktree versus HEAD; there are no per-turn snapshots or
editing/staging controls.

TDD proof at the pure parser, real public socket, GUI actor and native GTK seams
is recorded in the [core/server report](../../docs/qa/file-diff-review-2026-10-03/server-verification.md)
and [GUI report](../../docs/qa/file-diff-review-2026-10-03/gui-verification.md).
Final integration gates pass: build, fmt, clippy and **248 workspace tests**.
Clippy retains only the existing `audit.rs:111` warning. Fifteen display tests
are excluded from the headless suite; the new reader and existing Git-navigation
display tests pass separately.

Three independent reviews covered correctness, spec consistency and structural
quality against `edf10ce`. Accepted findings were corrected with regressions:
sorted empty-state removal, single truthful truncation notices, typed operational
I/O failures and inherited `GIT_DIFF_OPTS` overriding context. The latter's
[red](../../docs/qa/file-diff-review-2026-10-03/context-red.log)
and [green](../../docs/qa/file-diff-review-2026-10-03/context-green.log)
runs use the actual public IPC test under `GIT_DIFF_OPTS=--unified=0`.
Trusted Git normalization semantics are explicit in the spec, contract and
ADR-0016; timeout fixture markers are outside the checkout. No finding remains.

The [QA report](../../docs/qa/file-diff-review-2026-10-03.md) links all gate logs,
review resolutions and inspected light/dark/narrow/binary/incomplete GTK images.
Actual GTK interactions prove keyboard activation, numbered selectable text,
clipboard copying, Back focus/scroll, refresh and stale-response handling,
terminal retention and reader destruction after close. Real socket tests prove
Git normalization, literal paths, bounds, refusal of external symlink/FIFO
contents and descendant cleanup at the actual server deadline.

Delivery is one feature commit on `main`, followed by an ordinary push to origin.
