# ADR-0014: Codex interactive runtime belongs to a pane

Status: Accepted — 2026-09-30.

## Context
Codex 0.159 shares a detached daemon between interactive clients. Native hooks
snapshot that server environment rather than the current TUI environment. A
stale SIGNALTTY_PANE produces hook exit 1; an existing old pane receives wrong
state. Disabling daemon_auto_start alone still allows existing daemon reuse.

## Decision
At the common PTY execution boundary, direct local interactive Codex sessions
use the native --no-daemon option, including resume/fork. Probe support with a
bounded read-only --no-daemon --version invocation in a private process group
(cleaned up on completion/timeout); unsupported clients retain
original argv and get a setup notice. Existing explicit flags are preserved.
Utility/exec launches remain unchanged; explicit remote sessions get a notice.
Original argv and native resume commands stay persisted unmodified.

## Consequences
Each managed interactive pane owns its in-process app-server, preserving pane
and socket provenance. Higher per-pane resource usage is accepted. Hook commands,
provider config/auth/trust and shared daemons remain untouched. Manually typed
shell commands require the same flag; running clients need explicit exit/resume.
No guessed cwd attribution, per-pane hook-hash churn, daemon termination, or
silent acceptance of invalid pane ids. Upstream client-env forwarding can be
reassessed once it has a supported contract.

## Evidence
[Native flag and daemon exclusion](https://github.com/openai/codex/blob/rust-v0.159.1/codex-rs/tui/src/daemon_startup.rs#L25-L34),
[hook environment snapshot](https://github.com/openai/codex/blob/rust-v0.159.1/codex-rs/hooks/src/registry.rs#L71-L82).
