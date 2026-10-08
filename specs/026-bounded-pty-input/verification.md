# Verification and handoff

Base: PR 44 queue commit 199950c. Native Rust 1.94 toolchain.

## Diagnosis and reproduction

Real isolated one-worker server, Python raw PTY waits without reading; 256 KiB
input remains pending. pane.close returned but awaiting its input for one second
failed: `pane close must release the actual input operation: Elapsed(())`,
red in 1.29s. This matches Sonnet's independent PR 42 lifecycle finding.

Ranked causes before the fix: (1) retained blocking kernel write, (2) retained
writer turn, (3) lost RPC response. Prior debugger evidence showed the native
write; moving the registry lock/queue did not bound it. The selected design
replaces the syscall with nonblocking writes and bounded readiness waits.

## Focused proof

Four real IPC tests pass in 5.33s: close and KILL release a partially accepted
input; five-second expiry returns TIMEOUT; releasing the timeout child makes it
read exactly the reported accepted prefix; server.shutdown is followed by actual
owned process exit within two seconds (not a cleanup kill mistaken for exit).
TestServer's observation helper does not terminate the process.

Full verification passed on the pre-rebase tree: target/verification/full-evg5gqbt
(36 steps, 24 GTK tests); light/dark renders inspected. Current-main integration
requires another full run. Native Grok final review pending. Sonnet
design delegation was rate-limited and returned no review. No OpenRouter.

## Remaining integration

main advanced with PR 40's delivery/resume semantics. The isolation
PR was resolved against it with async consume/deliver/resume; queue and bounded
input rebased. The generic helper carries typed InputError in production while
retaining all four async helper regressions. Repeat full integration checks.
Synchronous Enter still waits under the Store guard, now bounded by the I/O
budget; spec 027 will make it try-only. Native provider sessions and desktop
accessibility are outside this server patch's proof.
