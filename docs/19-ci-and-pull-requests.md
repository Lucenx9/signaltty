# CI and pull requests

Use this procedure when publishing a change, handling review comments, recovering
failed checks or merging a PR. Local proof follows [agent development](17-agent-development.md)
and [workflow commands](18-agent-workflow.md); this guide covers the GitHub state.

## CI events and checks

[ci.yml](../.github/workflows/ci.yml) runs on PR opened/synchronize/reopened,
pushes to main and explicit manual dispatch. Contribution branch pushes and tags
do not automatically run CI. Opening or updating a PR tests GitHub's proposed
merge checkout; pushing main verifies the resulting main revision. There are no
path filters or draft skips.

| Job | Result required before merge |
|---|---|
| `workflow-lint` | actionlint 1.7.12 validates all GitHub Actions workflows |
| `rust` | Shared `scripts/verify.sh full`, including server QA, isolated GTK and benchmark |
| `minimum-rust` | Rust 1.92 checks the workspace and all targets with the lockfile |

The `verification` artifact contains summary.json, logs and screenshots even
when full verification fails. Read the failed step's log and inspect renders as
appropriate. Lint checks configuration; it does not prove event scheduling or
replace application verification. The exact ubuntu-26.04 label in
[actionlint.yaml](../.github/actionlint.yaml) supplements the linter's older list;
it does not select a self-hosted runner or suppress other diagnostics.

Concurrency groups use workflow, event and PR number or full ref. Different PRs
with equal fork branch names cannot cancel each other. A newer run in the same
group cancels the older run. PR, main-push and manual runs have distinct groups.
Tokens have `contents: read`; checkout does not persist credentials.

After this workflow is on main, explicitly verify an unopened branch with:

```sh
gh workflow run ci.yml --ref BRANCH
```

Manual dispatch needs a workflow on the default branch. Fork contributions can
require GitHub approval before running. Conflicting PRs need conflict resolution
before GitHub can run their merge checkout. Repository rulesets are separate
settings; these files define the checks and do not configure merge enforcement.

## Prepare and publish

1. Complete the selected feature's tasks, independent review and local full
   verification. Stage final source before full; check fresh delivery proof with
   `scripts/agent.sh check --phase delivery --feature FULL-SLUG --evidence RUN`.
2. Use the [PR template](../.github/pull_request_template.md). Describe concrete
   behavior and link the spec. Record the verified revision, actual commands,
   artifact, findings, limitations and rollback implications. `not run` and
   `pending` are valid results; they do not mean passed.
3. Push and create the PR. When using gh, write the exact description to a file
   and pass `--body-file`. Register the PR with the orchestrator when it supports
   PR linking. This applies to every layer in a stack.

## Review, recover and merge

1. Capture the current head before assessing comments or checks. Keep this value
   throughout the review and merge:

   ```sh
   reviewed_head=$(gh pr view NUMBER --json headRefOid --jq .headRefOid)
   ```

   Inspect conversation comments, review submissions and inline threads, including
   replies and resolved state. Use `gh-address-comments` when available. Evaluate
   each finding against source and project requirements; fix valid defects and
   record the reason for rejecting a suggestion. A bot's generic threshold does
   not become a project requirement. A reviewer out of quota has not approved.
2. Inspect the PR's check runs and workflow run identity. Confirm the head still
   equals `reviewed_head`; a change requires restarting the assessment.
   Use `gh pr view NUMBER --json headRefOid,statusCheckRollup,reviews`
   and `gh run view RUN_ID --json event,headSha,status,conclusion,jobs`.
   Require the latest PR run's three jobs to pass for the reviewed head. An old
   cancelled push run alongside a passing replacement is not an outstanding
   defect. A cancelled latest run, pending/failed check or unavailable log is not
   passing proof. If main advances, update and verify the proposed merge again.
3. For failed GitHub Actions checks, use `gh-fix-ci` when available. Read the log,
   reproduce the failure, form a diagnosis and make a bounded correction. Rerun
   only after examining the cause; repeated failure needs new evidence. Use
   existing session authorization for fixes and publishing. External reviewer
   outages or quotas are reported as limitations, not solved by retrying code.
4. After source changes, review the delta, stage final content and refresh local
   full/delivery proof. Push, then inspect CI and new comments for the new head.
   Finish pending reviews before claiming they are clear. Prefer orchestrator
   PR watching to a polling loop while waiting.
5. Merge only when authorized by the user/task and all latest checks and review
   decisions are settled. Preserve the reviewed revision in the merge request:

   ```sh
   gh pr merge NUMBER --squash --match-head-commit "$reviewed_head"
   ```

   Capture `reviewed_head` before assessing checks and comments, not after them.
   A changed head requires reassessment. Do not bypass checks with admin flags.
   Existing merge authorization persists; ask only if it was never provided.
6. Confirm merged state and merge commit. Stop orchestrator watching when handing
   work back. Start follow-up changes from updated main in a separate branch.
