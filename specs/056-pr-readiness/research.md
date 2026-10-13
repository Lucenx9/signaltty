# Research: Verified PR readiness

## Decisions

- Use a pure core projection instead of duplicating board rules in clients. It keeps GUI and orchestration task readers consistent without storing a derived status.
- Extend the existing gh observation instead of adding a GitHub API client. [gh pr view](https://cli.github.com/manual/gh_pr_view) exposes headRefOid, isDraft, mergeable, and mergeStateStatus. A read of repository PR #103 confirmed the installed CLI supports those fields.
- Require MERGEABLE and CLEAN, along with existing checks and review constraints. Unknown, draft, blocked, behind, dirty, and unstable observations cannot produce Ready to merge.
- Expire positive observations after five minutes. Preserve facts on refresh error but invalidate readiness immediately.
- Order writeback by attempt time and PR identity so overlapping requests cannot reverse a newer observation.

The [Agent Orchestrator investigation](../../docs/research/2026-10-12-agent-orchestrator.md) motivates richer facts. Its board rule is not copied because its Ready column also communicates ownership of the next step.

Alternatives considered: GUI-only changes leave API callers without a shared stage. A background observer and automatic feedback expand scope and require separate delivery policies. Clearing cached facts on failure loses useful diagnostics. None is needed for this repair.

The installed gh 2.86.0 [PR exporter](https://github.com/cli/cli/blob/v2.86.0/api/export_pr.go#L62) emits an array for rollup nodes and null when no nodes exist. The typed decoder accepts both explicit forms, while a missing rollup field remains a malformed observation. Real CLI reads verified supported head/draft/merge fields and normal CheckRun/StatusContext output shapes.
