# AGENTS.md review, 2026-10-30

This review compares signaltty's agent entry point with
[t3code's AGENTS.md at revision 32ecd717](https://github.com/pingdotgg/t3code/blob/32ecd71726fee16f335484c1f95cbce126d461f6/AGENTS.md)
and the official guidance below. These are documentation design choices, not
measured improvements in agent productivity.

## Sources and decisions

- [AGENTS.md format and guidance](https://agents.md/) describes build/test commands,
  project conventions and scoped instructions. Signaltty already has a root entry
  point and authoritative guides. Keep that structure and verify linked commands
  instead of adding another instruction hierarchy without a demonstrated need.
- [Anthropic's best practices](https://code.claude.com/docs/en/best-practices#keep-claudemd-short)
  recommend concise persistent instructions, task-specific procedures elsewhere
  and executable verification. Apply those writing principles across harnesses;
  keep existing repository checks and CI responsible for enforcement.
- T3code checks all affected entry points, clients, provider adapters and reverse
  states. Adapt that checklist to signaltty's GTK, CLI, typed Unix-socket protocol,
  server-owned state, events and agent/plugin hooks in the development guide.
- T3code calls out harm to the contributor's running instance. Signaltty already
  documents temporary sockets/state, owned-process cleanup and D-Bus isolation.
  Add a trigger in the entry point so agents reach those rules before launching
  probes, rather than discovering them only at final verification.
- T3code favors tests for behavior and existing fixtures. Clarify where signaltty
  regressions belong and give focused Cargo commands, while retaining the binding
  test-first requirement and full local delivery gate.

## Local gaps addressed

The previous startup instructions used `context --task <key>` without an inline
example or a description of valid keys. In this session, using a task title
returned `INVALID_INPUT`. The entry point now provides a valid `docs` example and
explains the distinction between routing keys and feature slugs.

The checkout's selected feature was `070-vte-terminal-interactions`, although this
request concerned documentation. Selection and completed tasks are useful
discovery context, but do not establish task relevance or fresh verification.
The entry point now makes that distinction explicit and links the existing
documentation-only exemption without changing the selected feature.

Publication rules were already in `docs/19-ci-and-pull-requests.md`, but the entry
point did not tell an agent when to read them. The completion checklist adds that
trigger, including the PR template, evidence and explicit unverified results.

## Practices not transferred

T3code's instruction to leave repository-wide checks to CI conflicts with
signaltty's constitution. Both local and CI verification still run
`scripts/verify.sh full`, including for this documentation change.

Its TypeScript/Effect, WebSocket origin, Electron/mobile, database-seeding and
provider-specific commands describe a different architecture. Signaltty keeps
its existing Rust/GTK test seams and helper-owned temporary state. This review
does not introduce live-data copying, new harness configuration or a new skill.

## Verification and limits

Check commands against their owning scripts, review consistency with the
constitution and guides, run the repository integrity check, and obtain fresh
full/delivery evidence before publication. The PR records the actual results.
No application behavior changes, so there is no new behavioral regression test
or visual acceptance claim.

To measure agent outcomes, use the existing
[evaluation protocol](../17-agent-development.md#evaluate-improvements-to-the-agent-workflow).
Useful cases are selecting `docs` without adopting an unrelated feature, finding
the isolated GUI probe rules before launch, and rejecting fast or stale evidence
as delivery proof. This review does not run those controlled agent trials.
