# Handoff: Agent workflow organization and automation

Branch/worktree: t3/automate-agent-workflows in this isolated checkout.
Base commit: 7fac1147705ceaf1b67bf52d5d34e00f85dcda6a.
Accepted scope: preserve historical paths/full slugs; central catalog and routing;
read-only discovery and phase checks; exclusive handoff; shared content/index proof.
Root owns final integration and publication. No remaining implementation ownership.

Implemented requirements FR-001 through FR-012. Canonical sources and CLI are in
docs/18-agent-workflow.md. Task status remains in tasks.md, lifecycle in the catalog.
Public CLI/evidence regression tests and independent Standards/Spec reviews passed.
Initial full verification: target/verification/full-5zz9zs9s, 45 GTK tests plus
server QA/benchmark, before the review corrections. That report is stale for the
final source. Final gate command is recorded below; inspect its actual result.

Next concrete step before publication:

```sh
scripts/verify.sh full --output target/verification/agent-workflow-reviewed-20261014
scripts/agent.sh check --phase delivery --feature 061-agent-workflow --evidence target/verification/agent-workflow-reviewed-20261014
```

Paid agent productivity trials and real-desktop manual accessibility were not run;
this tooling change makes no claim about those outcomes. Snapshot identity covers
checkout working/staged inputs, not external dependencies or temporary restored
writes between captures. Keep one source writer and stable inputs during checks.
