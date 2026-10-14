# ADR-0038: CI event identity and PR delivery

Status: accepted, 2026-10-14.

## Context

PR115 ran CI on its branch push and again on opening the PR; the second run
cancelled the first at the same head. Concurrency used branch names, which can
collide between forks. There was no repository PR template or GitHub delivery
procedure. Local full verification and content-bound proof already exist.

## Decision

Run automatic CI for PR opened/synchronize/reopened and main pushes, plus manual
dispatch. Group runs by workflow, event and PRnumber/fullref. Keep full and MSRV
checks and artifacts; add pinned actionlint as an independent workflow check.
Allow only the known reference runner label missing from its built-in list.
Use read-only repository permissions and omit persisted checkout credentials.

Use one PR template and a delivery guide for actual evidence, comment triage,
latest-revision check assessment and authorized merging against the reviewed head
after rechecking the reviewed base. The CLI's atomic guard covers the head only;
server-enforced base freshness requires branch protection or a merge queue.
Reviewer warnings require project-specific judgment. Existing authorization is
reused; no automatic merge or additional approval service is introduced.

## Consequences

Unopened branches have no automatic CI; manual dispatch becomes available when
the workflow is on main. Fork approval and merge conflicts can delay PR checks.
Path coverage, full/local parity and minimum Rust policy remain intact. Static
lint does not certify hosted scheduling. A new CI dependency uses a version tag,
not an immutable digest. Ruleset enforcement remains a repository setting.
