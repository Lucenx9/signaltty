# Plan

Build a unit regression at the real `PtyManager::input_async` call seam with
native PTYs and a runtime whose blocking pool has two threads. A raw child waits
for a test-owned release file, so the first large input blocks; another request
to it consumes the other pool thread on the old code. Another child's small
input must complete before release. Always release and drain before assertions.

Add an async per-writer gate. Acquire its owned guard before `spawn_blocking`
and move that guard into the closure. Keep the existing writer mutex for the
synchronous Enter path and original-PTY binding. No registry lock crosses an
await, and no Store lock is added. Document this refinement to ADR-0022.

Verify focused red/green, submit/decision regressions and `scripts/verify.sh full`.
Obtain independent review before publication and merge on green host CI.
