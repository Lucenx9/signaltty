# Verification: CLI pane identity and working directories

Date: 2026-10-14. Verified code revision: `eb64c2c`.

## Regression evidence

Both commands failed before the production fixes:

```sh
cargo test -p signaltty-cli --test cli cli_attach_keeps_the_named_pane_identity -- --exact --nocapture
cargo test -p signaltty-cli --test cli cli_relative_cwd_uses_the_callers_directory -- --exact --nocapture
```

Named attach remained open after close and child exit. Reusing a name after
rename redirected input. Opaque-ID attach passed as a control. Relative cwd
produced the server directory from `pwd`; absolute cwd produced the caller path.

After the fixes, all 20 CLI integration tests passed. Attach now covers opaque
ID, named close, named exit, and input after name reuse. Cwd covers real `pwd`
output from `new`, `pane spawn`, and `pane split` in a temporary checkout.

## Complete gate

```sh
RUST_TEST_THREADS=1 scripts/verify.sh full
```

Passed all 57 runner steps, including 748 ordinary workspace tests, server QA,
45 isolated GTK tests, and the refresh benchmark. Reviewed the 140 native
renders in contact sheets. Evidence is in
`target/verification/full-so6akcmb/summary.json` and its adjacent logs.

Serial execution limits simultaneous independent fixtures. Tests that exercise
concurrent requests internally still exercise that concurrency.

## Earlier parallel runs

`full-nlpsoabh` timed out in the existing PTY backpressure test. An isolated
rerun and five repeats of the exact failed executable passed. `full-36n1ptu6`
timed out in four existing screen-detection cases; one failed case passed in
isolation, and the complete serial run passed all four. Their causes remain
unconfirmed. These logs remain under `target/verification/`.

`full-zdpw75uf` exposed a three-second startup timeout in the new attach
fixture. Startup readiness now has ten seconds. Input and termination assertions
retain their three-second limits.

## Review

Standards review and Spec review found zero actionable defects. Both reviewers
also checked the separate startup and behavior deadlines. No temporary debug
instrumentation remains.
