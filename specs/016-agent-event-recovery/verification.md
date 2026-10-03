# Verification

Verified on 2026-10-03 against base `f70cd5125fa7ceb1310aef07637572de49f08cec`
with this feature's uncommitted changes. Final production code was unchanged after
the successful full run; subsequent edits only complete documentation.

## Gate and regression evidence

`scripts/verify.sh full` passed all 27 steps. Local evidence:
`target/verification/full-zkkwx60k/summary.json` and sibling logs/screenshots.
This includes formatting, locked dependency metadata, architecture, Clippy with
zero warnings, workspace build/tests, real server QA, all 15 ignored GTK tests in
isolated Xvfb/D-Bus processes, and the refresh benchmark. All seven native
permission tests pass, including concurrent answers, timeout and cancellation.

Development environment: Rust 1.94.0, GTK 4.18.6, libadwaita 1.7.6 and VTE 0.80.1.
`cargo +1.92.0 check --workspace --all-targets --locked` also passed locally,
verifying the declared minimum Rust version. Hosted CI repeats that check.

Three regressions were demonstrated failing before implementation and passing
after it:

- `new_events_after_restart_advance_the_previous_cursor`: restarted sequence 1
  previously followed an already observed higher cursor.
- `forward_output_gap_reconnects_and_replaces_the_screen`: a missing byte interval
  previously left the actor consuming a corrupt stream.
- `baseline_wait_ignores_old_done_and_accepts_fast_new_work`: the original API had
  no baseline, so an old Done could satisfy a wait for newly submitted work.

Additional tests cover corrupt sequence reservation, crashes without subsequent
journal records, sequence exhaustion, removed/truncated/legacy audit history,
rotation that cannot erase prior uncertainty, complete loss of journals with a
surviving sequence marker, filter-before-cap replay, numeric sequence holes,
cursor-ahead responses, exact replay/live handoff and lag-close. Wait tests drive
the CLI and isolated servers, including brief Blocked followed by Working,
independent attention/lifecycle baselines, repeated hooks, first-session discovery,
session A-to-B-to-A replacement, restart/resume identity and receiver cleanup on
EOF, next request and shutdown. Actor recovery also preserves PTY dimensions and
avoids duplicate later bytes or mutation replay.

The first full attempts exposed an outdated quality-checker fixture and a native
permission race. The fixture now owns its warning baseline; the permission waiter
consumes an already queued verdict before treating the accompanying state event as
cancellation. Both corrections are included in the successful full run.

## Render review and proof limits

All 17 saved native PNGs were inspected: light/dark palette, worktrees, changes,
file diff, search and zoom; narrow worktree/change/diff layouts; binary and truncated
diff states. Controls and text remain readable, narrow layouts fit, and diff
additions/deletions remain distinguishable. This change does not alter GUI styling.

The additional AT-SPI probe attempted under isolated Xvfb failed before its first
assertion: the installed Python binding does not make `Atspi.Accessible` iterable
(`qa-gui-reliability.py`, desktop enumeration). Its log is preserved at
`target/verification/full-zkkwx60k/map-accessibility.log`. No probe-owned GUI or
server remained; the prerequisite doctor was rerun after failure. This is not a
passing native desktop probe. GTK fixture tests, real Unix sockets and real PTYs
provide the proofs above; a compositor, clipboard, IME, folder chooser,
notification clicks and paid provider sessions remain outside them.

Recovery restores the current VT screen, not all disconnected scrollback. Journal
durability and cursor reservation are separate from snapshot durability. Journal
I/O failure closes the server rather than publishing an unrecorded state event.
Legacy uncertain disk history requires fresh snapshots; recent runtime ring
intervals can still be complete. Baseline capture must precede input, and callers
serialize concurrent writers when turn attribution matters.

## Standards

Independent review found journal evidence could be lost when a generation was
removed/truncated or legacy history was appended; this contradicted the documented
honest-replay contract. Persisted retention evidence and conservative legacy
handling resolve it. A later review found rotation could overwrite the uncertainty;
validation before rotation and a regression test resolve that case.

Review also flagged possible Primitive Obsession in generic JSON-based transition
inference, repeated-hook stamping and stale session-generation documentation.
Typed Store transition methods now own progress and publication, same-state hooks
do not advance progress, and the contract describes first discovery versus known
session replacement. The native permission race found by the gate was rechecked
after correction. Final review reported no remaining concrete defects in the
targeted changes.

## Spec

Independent review found incomplete audit history could claim completeness (FR-003)
and a brief matching outcome could disappear before a baseline wait observed it
(FR-006). Retention evidence rejects incomplete replay; bounded last-transition
stamps preserve matching outcomes independently of current state.

Final review found deleting both journal generations and their retention marker
while the sequence marker survived could incorrectly be treated as a fresh server.
Fresh-history detection now also requires the sequence marker to be absent; a
regression test proves the surviving-marker case reports unavailable history.
The reviewer confirmed this correction closes the final reported defect.

Final unresolved findings: Standards 0; Spec 0. Each axis's highest initial severity
was P1 (incorrect completeness claims), resolved and regression-tested.

## Publication

[PR #4](https://github.com/Lucenx9/signaltty/pull/4) targets `main` and is linked to
the originating T3 thread. GitHub reported it mergeable at publication. Hosted
`rust` and `minimum-rust` checks were running when first inspected; the PR shows
their current status. CodeRabbit skipped its review under the repository's OSS
policy; the independent reviews recorded above were performed locally.
