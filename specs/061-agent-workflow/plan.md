# Plan: Organized agent workflow

## Grounding and constitution check

Setup owns installation, Spec Kit owns feature selection/scaffolding, and
verify.py owns executed checks. ADR-0017 establishes the stdlib Python runner
and isolated application QA. Extend those boundaries. Feature 061 requirements
are resolved; test at the existing script CLI/evidence seams, test-first.
No application, IPC or GUI change, historical spec moves or vendor modifications.
The full delivery gate remains binding. No constitution amendment is required.

## Usage

```sh
scripts/agent.sh status --feature 061-agent-workflow --json
scripts/agent.sh context --task ipc --json
scripts/agent.sh next --feature 061-agent-workflow --json
scripts/agent.sh check --phase repository
scripts/agent.sh check --phase implementation --feature 061-agent-workflow
scripts/agent.sh check --phase delivery --feature 061-agent-workflow --evidence target/verification/full-RUN
scripts/agent.sh handoff --feature 061-agent-workflow --output target/handoffs/session.md
scripts/agent.sh index --write
```

## Shape and ownership

Use scripts/agent.sh as a shell launcher, scripts/agent.py as the repository
workflow owner, and scripts/verification_evidence.py as the shared provenance
owner. Avoid separate classes/modules for loading, validation and each phase.
The CLI imports evidence policy directly; verify.py imports only evidence policy.

.agents/workflow.json owns routing and the full-slug feature catalog. Each entry
records lifecycle and explicit phase-check enrollment, with an optional successor.
Historical entries start unreviewed and unenrolled, not retroactively completed.
Tasks/requirements/phase are derived from spec.md, plan.md and tasks.md; tasks
for enrolled features carry [FR-###] references. A recorded completed lifecycle
is a human/agent claim, displayed independently from observed verification.

Spec Kit --paths-only --json with no-persist resolves selection. Boundary checks
require a full slug or direct child of specs and reject symlink escapes. Read
commands never write pointers. No selection gives repository context/next setup.

Index generation projects catalog + artifact presence into specs/README.md and
routing into a marked docs/15 block. Do not include checkbox counts in generated
indexes, so routine progress updates do not stale the index. status derives counts.
Repository checks validate catalog coverage, routes, authoritative local links,
canonical verification symlinks, generated blocks, and installed generated command
copies when present. Historical incomplete tasks are not repository failures.

Planning requires a clarified spec; implementation adds plan/tasks/checklists
and complete requirement-to-task references; delivery adds all implementation
tasks checked and fresh successful full/desktop evidence. Explicit doc-only or
one-line scope preserves constitutional spec exemptions, reports scope, and still
requires the shared delivery gate. Phase checks inspect; they never run tests.

Handoff creates a new file exclusively and includes facts, open task references,
evidence status, next step and a separate editable decisions/ownership section.
Default output under target is ignored; writing a handoff inside tracked source
invalidates old evidence just like another edit. Never overwrite handoff or tasks.

## Provenance contract

capture_checkout(root, output=None) returns a versioned JSON-serializable
snapshot containing a content digest, index digest, root, revision and file count.
Digest framing includes path/type/mode/content for tracked + nonignored untracked
working files and staged blob/mode records. Git failures are explicit errors.
Commit identity is recorded but not part of equality: committing identical staged
and working content keeps proof applicable. Ignored evidence is outside inputs.
Custom nonignored output inside the repo is rejected rather than excluding source.
Evidence root must match checkout; no caller-chosen source exclusion is accepted.

verify.py records provenance.before and provenance.after plus unchanged. Changed
inputs during checks cannot pass. Evidence assessment requires passed full/desktop,
unchanged before/after, matching current snapshot and completed successful steps.
Legacy summaries, doctor, interrupted/failed runs and malformed provenance report
unverified/failed/stale states rather than fresh proof.

## Synthesis and tradeoffs

The central-catalog alternative is the base: it exposes missing entries and makes
lifecycle/enrollment visible in one place. Graft the other candidate's [FR-###]
traceability and exclusive-create handoff. Reduce both sketches to two Python
owners. Reject per-feature metadata because accidental removal could silently
unenroll a feature, and reject mutable phase/task ledgers as duplicate state.
The independent judge result is recorded in research.md. Claude/Grok candidates
could not run due API rate limit/usage balance; two GPT candidates were compared.

Accept recomputed hashes and a reviewed central catalog for honest facts without
caches or mutable phase state. An automated check cannot prove semantic acceptance
or visual quality. Existing native/manual and paid-provider proof limits remain.

## Verification and integration

Root owns agent.py, metadata, docs, setup-agent.sh and CLI tests. A separate
implementation agent owns verification_evidence.py, verify.py and provenance tests.
Their shared contract is above; no shared file writers. Integrate the repository
check in verify.py so CI uses identical code. Run focused Python CLI tests each
slice, tooling suite, actual status/context/check/handoff flows, then verify.sh full.
Review standards and spec independently against base 7fac114 before publishing.
Extend task bank/protocol to compare variants; do not run paid trials or claim a
measured quality improvement. Publish and link the PR on the existing branch.
