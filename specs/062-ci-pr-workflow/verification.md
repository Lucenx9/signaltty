# Verification procedure and observed evidence

Base: fd4e4ff, the squash merge of PR115. Its latest PR full/MSRV CI and completed
Codex/CodeRabbit reviews were inspected before the authorized merge. Copilot
quota exhaustion and the generic docstring warning were not passing review claims.

Observed focused checks:

- Invalid event fixture: actionlint exits1 and reports unknown pull_requst event.
- Actual workflow: native actionlint1.7.12 exits0 after explicit runner allowance.
- Upstream Docker1.7.12 image: exits0 against an isolated ordinary checkout with
  GitHub-style readable file permissions. The local worktree's restrictive mode
  initially prevented the image's guest user from reading it; only the ignored
  validation fixture's permissions were adjusted.
- Repository and implementation phase checks: pass; generated index is current.
- Independent read-only grounding: confirms event/group semantics and caveats.

Concurrency examples from the chosen GitHub expression:

| Event and identity | Group |
|---|---|
| PR116, branch feature/fix | ci-pull_request-116 |
| PR117, same branch name in another fork | ci-pull_request-117 |
| Main push | ci-push-refs/heads/main |
| Manual main run | ci-workflow_dispatch-refs/heads/main |

These examples check the documented identity; they do not simulate GitHub's
scheduler. Workflow Docker tags are version pins, not immutable image digests.
Hosted execution is checked after publication. No productivity measurements,
paid-provider tests or real-desktop accessibility acceptance were run.

Final source gate:

```sh
scripts/verify.sh full --output target/verification/ci-pr-workflow-20261014
scripts/agent.sh check --phase delivery --feature 062-ci-pr-workflow --evidence target/verification/ci-pr-workflow-20261014
```

Read the actual versioned summary and delivery result for completion. The PR body
records those outcomes after running them; this procedure is not successful proof.
