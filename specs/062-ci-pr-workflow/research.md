# CI and PR grounding, 2026-10-14

Observed baseline: PR115 head4f051a8 had push run38567648569 cancelled at PR
creation, then PR run38568044486 repeated the jobs and passed. CodeRabbit and
Codex completed without actionable findings; Copilot could not review due quota.
CodeRabbit's generic docstring threshold is not a project quality requirement.
PR115 was merged at fd4e4ff with explicit user authorization and matched head.

Decision: automatic pushmain and pull_request, plus manual dispatch. GitHub
supports per-event branch filters; multiple matching events create separate runs.
Concurrency groups include event and PRnumber/fullref rather than headbranchname.
[GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)
and [event semantics](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows).
Do not claim lint proves hosted event scheduling. Dispatch needs the workflow on main.

Decision: actionlint1.7.12 official Docker image, with the known Ubuntu26.04 label
configured explicitly. Local1.7.12 initially rejects only that label. The upstream
image includes ShellCheck/pyflakes and accepts version tags.
[actionlint usage](https://github.com/rhysd/actionlint/blob/main/docs/usage.md),
[configuration](https://github.com/rhysd/actionlint/blob/main/docs/config.md).

Decision: one .github PRtemplate, surfaced by GitHub's creation flow. It points to
authoritative local proof and current review/check state rather than generated
boilerplate assertions. [GitHub template guide](https://docs.github.com/en/communities/using-templates-to-encourage-useful-issues-and-pull-requests/creating-a-pull-request-template-for-your-repository).
No measured runtime savings or reviewer quality claims are made.

An independent read-only grounding review confirmed the design. Caveats include
fork approval, merge conflicts preventing PR runs and dispatch requiring main.
An exact label allowance changes lint recognition, not runner provisioning.
The source report is target/ci-grounding.md.
