# Design synthesis and research

The [web research](../../docs/research/2026-10-14-agent-workflow-automation.md)
records primary sources, contrasting empirical results and GitHub examples.
No productivity improvement is asserted by this change.

Architect grounding traced setup, Spec Kit selection and the verification runner.
Two independent GPT designs were compared. The cross-judge scored the central
catalog alternative 20/25 versus 18/25 for per-feature sidecars. The central map
makes removed or missing enrollment visible; a sidecar's absence could mean either
historical work or accidental loss. Graft FR references, public CLI diagnostics,
exclusive-create handoff and the shared repository check. Reduce the module map
to agent.py and verification_evidence.py to keep each policy in one owner.

HEAD is descriptive metadata, not proof identity. Hash actual working and staged
content; a commit of the same inputs retains proof. Preserve historical slugs and
paths, do not infer lifecycle from task checkboxes, and reject nonignored evidence
output rather than accepting a source exclusion. Handoff reports point-in-time
facts and rechecks proof after writing.

Claude grounding returned “Claude API rate limit reached. Try again later.”
Grok's design seat returned “API error (status 402 Payment Required): Grok Build
usage balance exhausted”. Neither produced a design. Same-provider candidates
and an independent GPT judge completed the comparison; no cross-family agreement
is claimed. The design matches ADR-0017 and is recorded in ADR-0037.
