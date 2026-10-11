# Verification

Verified 2026-10-11 in isolated worktree `/tmp/signaltty-osc-grok`.
Code revision: `471b9b3`. Base: `fc1a517`. Later commits update documentation only.

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
