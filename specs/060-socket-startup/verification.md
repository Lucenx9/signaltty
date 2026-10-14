# Verification: Socket startup preservation

Base: `45ea97435849d37cc29cddaca0eb4d9dc2578567`. Date: 2026-10-14.

## Diagnosis

The real server replaced both a regular file selected as its socket path and
an inaccessible live socket. The minimized fixtures require only an occupied
path and production startup; no workspace or PTY is needed.

`cargo test -p signaltty-server --lib server::tests::startup_ -- --nocapture`
failed before the fix: one test passed, three failed. The failures were lost
file contents, a changed live-socket inode, and a removed symbolic link.
Separate Unix `connect` and `bind` probes preserved the same regular file,
ruling out either syscall as the operation deleting it. The cause was startup's
unconditional removal after every failed connection.

After the fix the five initial startup tests passed in 0.04 seconds. An additional
directory-preservation test was added for the full gate. The positive test uses
production startup and real `server.status` / `server.shutdown` IPC for both
absent and abandoned socket paths.

The original scenarios were also rerun against the rebuilt server executable:
both refused startup with a nonzero exit, preserving the file bytes and the
inaccessible listener's socket inode. Owned subprocesses and temporary paths
were cleaned up after each scenario.

## Standards

Standards reviewer: no actionable findings. Filesystem handling remains at the
server boundary; temporary fixtures are isolated, and the repeated test setup
does not justify an abstraction. No new architecture or protocol decision needs
an ADR.

## Spec

Spec reviewer: no findings. Path-entry inspection, preservation, connection-error
handling and real IPC recovery satisfy the specification. There is no scope creep
or additional concurrency guarantee.

Both reviews were read-only and did not run tests. The parent ran verification.
Standards findings: 0. Spec findings: 0.

## Full gate

The initial `scripts/verify.sh full` run passed the startup regressions but failed
on four screen-detection timeouts. Evidence is in
`target/verification/full-zeq2863_/`. Isolated failing cases passed; running their
24-test binary together reproduced variable timeout failures. Diagnostic reads
showed expected output, process names, and matching rules at those timeouts.

With `TOKIO_WORKER_THREADS=1`, the unchanged screen-detection binary passed all
24 tests twice, including a run after removing temporary instrumentation. Merely
serializing tests with the default server worker count still produced a timeout.
This establishes worker-count-sensitive timing on this four-CPU host; it does
not establish a separate parser or process-scoping defect.

`TOKIO_WORKER_THREADS=1 scripts/verify.sh full` passed; evidence is in
`target/verification/full-fwemezp2/`. Tooling tests, formatting, architecture,
Clippy, warning policy, build, all 752 ordinary workspace tests and server QA
passed, followed by all 45 isolated GTK tests and the refresh benchmark. All 57
recorded verification steps passed. The generated project-sidebar light and dark
renders were inspected with no obvious issue. No assertions or test deadlines
were changed.

## Limits and prevention

This is a bounded bug audit, not a claim that the repository contains no other
defects. The fix adds no lock against competing startups or external path
replacement. Permission-sensitive tests require the unprivileged environment
already enforced by the full runner. Existing manual GUI/provider proof limits
in `docs/17-agent-development.md` remain applicable.

The startup regressions would have caught these defects before release. The
existing production seam was sufficient; no architecture change is needed.
All reproductions used owned temporary paths, and no debug logging was added.
