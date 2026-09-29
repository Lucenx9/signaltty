# ADR-0013 — Shared installer and automatic direct-agent setup

Status: Accepted (2026-09-30).

The user approved configuring hooks automatically when starting an agent from Signaltty. CLI-only explicit installation currently leaves new agent workspaces without semantic status. Reusing its filesystem code in pure adapters would violate docs/11; invoking CLI as a subprocess adds discovery, protocol and timeout failure modes.

Add signaltty-integration, an OS-facing library shared by CLI and server. Keep signaltty-agent pure. The common PTY boundary prepares the provider identified by actual direct argv before execution, covering new, split and resume. Shell launches do not install every provider; agent hints and ambiguous agent aliases cannot authorize configuration writes. Installation failures are visible additive reports and do not stop terminal launches.

Preserve unrelated configuration through strict parsing, command-level managed ownership, per-file advisory locking and atomic writes. Reject foreign plugin collisions. Codex trust stays with its native review flow; no trust-state mutation or bypass flags. Existing manual install/uninstall/status remain available. Advisory locks cover Signaltty writers, not other applications.

Persist only the three supported explicit provider-directory overrides in
AgentInfo.config_env so official resume retains the same config and hooks.
Normalize relative roots against launch cwd; never persist arbitrary env/tokens.
