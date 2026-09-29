# Codex pane runtime isolation

Status: Complete. Main implementation 1a2d5ce published; CI 36647316905 passed. Bug reported 2026-09-30: `hook exited with code 1`.

## Problem
Codex 0.159.1 shares a detached app-server whose environment retains the pane
and socket of its first client. A new TUI inherits the correct current pane,
but hooks run in the daemon with the stale pane: NO_SUCH_PANE, or worse an
update to another still-live pane. Native hooks capture the hosting server env.

## Requirements
1. Direct local interactive Codex launch, split, fork and official resume run
   with pane-local backend context; semantic events affect only their pane.
2. Preserve supplied arguments, saved resume argv, config roots, authentication,
   hook command bytes and trust. Never stop or reconfigure shared daemons.
3. Do not inject TUI-only flags into exec, utility commands or explicit remote
   launches. Respect option values and the prompt separator.
4. Older/unrecognized Codex must remain launchable with a clear integration
   notice when pane-local runtime cannot be configured. Capability discovery is
   bounded and does not request a model or execute hooks.
5. Document that manually typing Codex in a shell requires `--no-daemon`; existing
   clients cannot be retroactively isolated. Provide resume guidance.
6. Retain strict invalid-pane hook reporting; do not hide incorrect attribution.

## Acceptance
Real PTY fixtures exercise installed hooks in two panes, split and resume;
Working/Done attribution is independent. Argument matrix covers utility,
remote, prompt text, options and idempotence. Unsupported/hanging capability
probes preserve launch and show notice. Required workspace gates pass.

No new IPC methods or model changes; no UI redesign or shell wrappers. Native
remote hook attribution and upstream per-thread environment forwarding are
outside scope. No constitution deviations or unresolved clarifications.
