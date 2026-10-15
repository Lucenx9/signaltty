# Plan: Diff revision and filename collisions

## Context and constitution

Use the existing Rust server diff interfaces and real Git fixtures. Read docs/02, docs/17, docs/19, ADR-0016 and ADR-0033. Follow test-first diagnosis. No new architectural choice, data model or protocol change requires an ADR.

## Implementation

Terminate the revision arguments with `--` in `git_diff`, `task_git_diff` and `turns::changed_files`. Do not refactor the shared helpers or parser. Existing selected-file diffs already use the separator.

## Verification

First add and run a server integration test using the existing workspace IPC and public task diff function. Observe failure, then fix those two calls. Next add a real-snapshot test at the existing public turns interface, observe failure and fix that call. Test collisions with both tree revisions. Review the diff independently, stage final source, run scripts/verify.sh full and check delivery evidence before publishing. The missing revision separator is the root cause and belongs in the PR description.
