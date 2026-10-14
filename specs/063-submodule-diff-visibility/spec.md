# Feature specification: Submodule commit changes remain reviewable

**Feature**: `063-submodule-diff-visibility`
**Branch**: `t3/repo-bug-fixes-3`
**Created**: 2026-10-14
**Status**: Clarified
**Input**: Continue finding reproducible bugs, fix them and merge verified PRs.

## User scenarios and testing

### User story 1: Review a changed submodule commit (P1)

As a developer supervising an agent, I can see that a submodule changed commit
in a workspace or task and read its old/new commit identifiers.

**Independent test**: Advance a local submodule, set Git's ignore-submodule
configuration, then request the summary and selected patch through real IPC.

**Acceptance scenarios**:

1. A submodule pointing to a different commit appears in the HEAD summary and
   its selected patch shows removed and added Subproject commit lines.
2. The same result holds for the latest-turn comparison and a task's recorded base.
3. Global diff ignore settings, per-submodule settings and .gitmodules ignore
   settings cannot hide a changed commit from explicit Signaltty review.
4. An unchanged submodule remains unchanged when it contains only untracked files.

## Requirements

- **FR-001**: HEAD workspace and task summaries MUST include changed submodule
  commit pointers regardless of Git ignore-submodule settings.
- **FR-002**: Selected-file workspace and task reads MUST display the old/new
  submodule commit pointers regardless of those settings.
- **FR-003**: Latest-turn summaries and selected patches MUST include submodule
  commit changes made after the baseline was captured, even when later staged.
- **FR-004**: Reads MUST preserve the user's Git configuration and index and
  retain existing patch parsing, bounds and canonical clean conversions.

## Edge cases

Unchanged pointers, staged pointer changes, .gitmodules settings, local
submodule settings, and diff.ignoreSubmodules. Untracked-only dirt inside submodules retains Git's ordinary ignored behavior.
Changes inside submodules are
represented by Git's short gitlink patch; recursive file review is outside scope.

## Success criteria

- SC-001: A deterministic real-IPC regression fails before the fix and passes after it.
- SC-002: HEAD, turn and task comparisons include the expected old/new commit lines.
- SC-003: Shared full verification and fresh delivery proof pass for final source.

## Assumptions and clarification

Existing specs 014, 020 and 041 require explicit review against the selected base.
Git ignore configuration is a presentation preference, not a change to that base.
No recursive submodule traversal, new method, persistence, GUI or schema change
is requested. Requirements are resolved from the existing review contracts and
this reproduced false unchanged result; no unresolved questions remain.
