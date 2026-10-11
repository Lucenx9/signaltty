# Verification

Verified 2026-10-11 in isolated worktree `/tmp/signaltty-osc-grok`.
Code revision: `471b9b3`. Base: `fc1a517`. Later documentation commits record publication; the bot-review correction below
has its own full verification run.

## Test-first evidence

- `cargo test -p signaltty-term progress_is_not -- --nocapture` first failed:
  busy/idle OSC progress appeared as Notify events. After the scanner change,
  every two-chunk split with BEL/ST termination passed.
- `cargo test -p signaltty-agent progress_region` first failed because the
  matcher had no progress input. The new region reads present evidence only,
  rejects line counts, and cannot match absent evidence with an empty regex.
- `cargo test -p signaltty-server --test screen_detection progress_rules_finish`
  first timed out waiting for working. After current-PTY evidence was connected
  to classification/explain, the real PTY reached working then done without
  progress notifications.
- `cargo test -p signaltty-agent grok_progress` first returned no rule. Bundled
  Grok/grok-build rules now classify progress and blockers and enter launcher discovery.
- Targeted results: 75 agent tests passed; 47 terminal tests passed, one unrelated
  performance test ignored. New real-PTY scenarios verify Grok working, blocked,
  resumed work and completion, hook dominance, plain terminals and evidence
  removal after exit, respawn and restart.

## Full delivery gate

`scripts/verify.sh full` passed. Evidence:
`target/verification/full-j0t91a9h/summary.json` and sibling logs/screenshots.
693 workspace tests passed, zero failed. All 33 isolated GTK tests passed.
Tooling tests, fmt, architecture, Clippy/warning policy, build, server edge-case
QA and refresh benchmark passed. The first full attempt stopped at one new
Clippy collapsible-if warning; it was fixed before this successful run.

Reviewed `screenshots/changes-light.png` and `changes-dark.png`: text and controls
remain readable and workspace selection is visible in both renders. No GTK
implementation change was required for launcher discovery.

## Independent reviews

`code-review` Standards and Spec axes reviewed `fc1a517...471b9b3`.
Standards: zero documented violations or significant smells in both rounds.
Spec: one ambiguous reset guarantee in round one; clarified to managed PTY
replacement and documented shared evidence across foreground programs in one
shell. Round two: zero findings. No unresolved objections.

## Proof limits

Local `grok --version` returned 1.0.50. No authenticated/paid Grok turn was run;
the functional proof uses emitted protocol/UI fixtures through real PTYs.
This PR adds detection, not native Grok hooks or a resume installer.
Foreground program changes inside one shell retain the latest OSC progress until
it is overwritten. Reset is guaranteed for managed PTY exit, close, respawn and
cold server restart. Live accessibility/IME and notification clicks were not run.

## PR #92 bot-review correction

Codex reported spinnerless active status lines (P1) and Grok's terminal-brand
OSC gate (P2). The non-OSC fallback now matches a trailing `[stop]` chip
without requiring braille. The real PTY fixture uses `TERM_PROGRAM=signaltty`,
`TERM=xterm-256color` and emits no OSC progress. Before the regex correction,
it timed out waiting for working; after correction it passed working, no early
done, blocked, resumed working and completed unread assertions.

The bundled title/footer matrix also includes the upstream spinnerless fixture.
OSC remains optional; no terminal identity spoofing was introduced. Independent
Spec and Standards reviews of this correction each returned zero findings.

Sources: [spinnerless upstream reproduction](https://github.com/herdrdev/herdr/issues/3120),
[Grok terminal-brand gate](https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/src/notifications/progress.rs).

The correction passed `scripts/verify.sh full` with evidence at
`target/verification/full-3baspa4g/summary.json` (base `b38667d`, correction dirty
at run start). All workspace tests and 33 isolated GTK tests passed; tooling,
formatting, architecture, Clippy/warning policy, build, server QA and refresh
benchmark passed. Reviewed the new light/dark changes-panel renders: selection,
text and controls remain readable.
