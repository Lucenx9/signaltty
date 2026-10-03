# Independent adversarial review

Reviewed working changes against `edf10ce`, new core/server/GUI sources, IPC tests, spec/plan/contract and ADR-0016. Applied interrogate correctness and code-quality rubrics. Existing accepted findings were excluded.

## Act on

**Fixed context can be overridden by inherited `GIT_DIFF_OPTS`.** `crates/signaltty-server/src/file_diff.rs:134` requests `--unified=3`, but the process builder at lines 265–294 inherits Git environment. Git gives `GIT_DIFF_OPTS` priority over the command-line option. Reproduced with the exact production flags, comparing current `crates/signaltty-gui/src/actor.rs` to `edf10ce`: normally its first hunk is `@@ -15,6 +15,7 @@` with unchanged context; prefixing the same invocation with `GIT_DIFF_OPTS=--unified=0` produces `@@ -17,0 +18 @@` and no context. That violates spec story 1 acceptance 1 and contract/ADR's fixed three-line comparison. Remove `GIT_DIFF_OPTS` from this child's environment and cover the override at the Git/IPC seam. Small correction; no payload redesign is needed.

## Clean-filter judgment

**Clarify the contract; preserve Git normalization.** Tracked reads invoke ordinary `git diff` at `file_diff.rs:124–141`, as the existing summary does. The deadline test explicitly configures `filter.slow.clean = sh slow.sh`; its fixture writes `filter.pid` and `sleeper.pid` at `tests/file_diff.rs:509–534`, proving execution of configured normalization and possible filesystem effects. These are repository files, not Store workspace state; the new handler emits no events, persists nothing and touches no terminal widgets.

A blanket filter prohibition would change Git's normalized comparison and diverge from the summary, including common required filters. The accepted design says Git owns normalization and the implementation tests it. Retaining trusted local clean/process filters is appropriate scope, provided spec FR-009, IPC documentation and ADR consequences explicitly distinguish application-initiated mutations from effects of trusted Git configuration. Current unconditional read-only prose overstates the guarantee. External diff/textconv remain deliberately disabled and filter descendants remain subject to the deadline/process-group bound. No additional filter-policy change is warranted for this increment.

## Remaining result

No further proven correctness or structural blocker found. Pure parsing stays in core; unsafe file acquisition and bounded subprocess handling stay in server; GUI response checks include alive, generation, selected path and reader visibility. The extracted dialog controller is cohesive, and no extra abstraction or wholesale refactor would improve this small feature enough to justify expansion.

## Resolution

The child environment now removes GIT_DIFF_OPTS. The public tracked-file test asserts an unchanged context line; it failed under GIT_DIFF_OPTS=--unified=0 before the fix and passes afterward (context-red.log and context-green.log). Spec FR-009, ADR-0016, IPC contract and documentation now state the trusted Git normalization boundary. Timeout fixture process markers live in the isolated integration directory outside the checkout. Both findings are resolved.
