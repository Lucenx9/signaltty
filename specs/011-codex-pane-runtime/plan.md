# Plan

Grounded official rust-v0.159.1 sources: hooks registry captures server env;
--no-daemon excludes both existing-daemon discovery and daemon auto-start.
--disable daemon_auto_start does not exclude reuse. Two independent candidates
compared embedded native mode with per-thread forwarding/private app-server:
forwarding is unavailable upstream; private supervision duplicates native mode.
Select supported --no-daemon at the shared server PTY boundary (ADR-0014).

Keep original stored argv and pure adapter resume contract unchanged. One private
server module classifies local interactive invocation, probes executable support
using --no-daemon --version (stdio discarded, deadline 500ms), and supplies
execution arguments plus a notice. Version probes use a private process group
so timeout cleanup also stops wrapper descendants. Value-taking options share
one list; known native boolean/short-attached options are supported, unknown
options preserve argv with an explicit notice. Explicit existing --no-daemon needs no probe.
Remote invocations remain unchanged with a disabled-integration notice. Utilities
remain unchanged. No hooks/trust/config rewrite for runtime isolation.

A single bounded synchronous probe adds at most 500ms to the existing serialized
spawn ownership transaction. Avoid a new companion process or runtime cache.
Actual Codex version probe is read-only and makes no model requests.

TDD: public server/PTY routing fixture red first, then argument and capability
regressions; use installed hook commands and lifecycle state as observable seams.
Review standards and spec independently. Workspace fmt/build/test/clippy gates;
no GUI changed, so reuse prior native GUI verification.

Architect phases: Ground complete; two candidates complete; autonomous selection
within prior fix/push authorization; implementation complete; final assessment preserves the chosen shape after
fixing two review findings (option coverage and wrapper cleanup).
