# Feature Specification: CI and pull request workflow

**Feature Branch**: `t3/ci-pr-workflow`
**Created**: 2026-10-14
**Status**: Clarified
**Input**: Complete PR #115, then improve CI and PR handling and open a follow-up PR.

## User scenarios and testing

### US1: Verify a proposed change once (P1)

A contributor pushes a branch and opens or updates a PR. Verification runs for
the proposed merge, without cancelling a duplicate check of the same contribution.
Acceptance: a branch push alone starts no automatic run; PR creation and updates
run the complete checks; merging to main runs verification of the resulting main.
Contributors can explicitly request verification of a branch before opening a PR.

### US2: Keep independent contributions independent (P1)

Two contributors use the same branch name in different forks. Updating one PR
must not cancel the other's checks. Acceptance: concurrency identity differs
between PR numbers, main pushes and manual runs; a newer run replaces only an
older run in its own group.

### US3: Prepare and deliver reviewable changes (P2)

A contributor describes the problem, resulting behavior, verification and risk.
An agent reads comments and checks before merging the reviewed revision.
Acceptance: the PR template points to the delivery procedure; the procedure
distinguishes failed, pending, cancelled and successful runs and records review
limitations without claiming unavailable reviewers approved the change.

## Requirements

- FR-001: Run automatic verification on PR creation, update and reopening, and on
  main pushes. Do not automatically run it on contribution branch pushes or tags.
- FR-002: Allow manual verification on a selected branch.
- FR-003: Group concurrency by workflow, event and PR number or full ref;
  cancel superseded runs within the same group only.
- FR-004: Preserve the full verification, minimum Rust check, reference environment,
  timeouts and failure artifact upload. All changed paths remain covered.
- FR-005: Verification uses read-only repository token permissions and does not
  retain checkout credentials. It does not publish or merge changes automatically.
- FR-006: Validate workflow syntax, expressions and action arguments in CI with a
  pinned linter; allow the repository's actual reference runner label explicitly.
- FR-007: Provide a short PR template for behavior, related spec, actual verification
  and limitations, reversibility and affected components.
- FR-008: Document comment triage, failure diagnosis, latest-revision check selection,
  explicit merge authorization and revision-bound merging; route agents to it.

## Edge cases

Equal branch names in different forks; new commits during review; cancelled old
push runs alongside a passing PR run; fork approval requirements; reviewers out
of quota; workflow conflicts with main; a manual run before the new workflow is
on main; a linter whose built-in runner list predates the reference environment.

## Success criteria

- SC-001: Event and concurrency examples yield one automatic PR run per triggering
  event, distinct groups for distinct PRs and no branch-push duplicate.
- SC-002: Valid workflows pass lint; a misspelled event or expression fails lint.
- SC-003: Shared full verification and delivery proof pass for the final source.
- SC-004: A reviewer can find the delivered revision, actual proof and acknowledged
  limitations from the PR and delivery guide without guessing from aggregate status.

## Assumptions and clarification

Scope is the single CI workflow and contribution guidance. Main is the default
branch. GitHub's PR merge checkout is retained. No repository ruleset changes,
release automation, paid review configuration or app behavior changes are needed.
Review tools advise; project requirements determine whether a warning is binding.
The authorized first PR was merged after latest CI and completed reviews passed.
Requirements are resolved from that observed workflow and the user's instruction.
