# Acceptance and verification

## Requirement-to-proof map

| Requirement | Proof |
|---|---|
| FR-001 environment | Container build, container doctor, repeated setup; doctor CLI regression for missing tools |
| FR-002 unified entry | `scripts/verify.sh full`; failure/termination/evidence-overwrite CLI tests |
| FR-003 CI evidence | `.github/workflows/ci.yml` uses full and always uploads artifacts; inspect hosted PR checks |
| FR-004 boundaries | Metadata CLI fixtures cover allowed, forbidden aliased and transitive toolkit dependencies; actual workspace check |
| FR-005 warnings | Diagnostic fixtures cover known warning, moved warning, new warning, unlocated warning and missing build completion |
| FR-006 isolated GUI | All 15 discovered ignored tests run separately with Xvfb/D-Bus; native screenshot artifacts |
| FR-007 canonical guide | Harness-link regression resolves Claude/Cursor/Copilot to `.agents/skills/verify-signaltty` |
| FR-008 routing/handoff | `docs/15-agent-skills.md`, `docs/17-agent-development.md`, constitution 1.1.0 and ADR-0017 |
| FR-009 evaluation | Five fixed-baseline tasks and result template under `docs/qa/agent-evaluation`; no quality uplift claimed |
| FR-010 visual rubric | `docs/17` acceptance matrix and explicit manual limits, this requirement map |
| FR-011 Rust floor | `cargo +1.92.0 check --workspace --all-targets --locked`; separate CI job |

## Local results

- Container image built successfully from Ubuntu 26.04; doctor passed inside it.
- Rust 1.92.0 compiled the whole workspace and all targets successfully.
- Full verification passed with ordinary workspace tests, build, fmt, architecture,
  warning policy, server QA, 15 separate GTK tests and refresh benchmark.
- Tooling CLI regressions cover failure propagation, cancellation and evidence retention.
- Upstream skill check validated 72 installed skills against cached pinned sources.
- A repeated host workspace run failed the pre-existing worktree event source-ID
  assertion once; 20 isolated repetitions passed. No retries or assertion suppression
  were added. The runner now explicitly builds binaries before integration tests.

Host evidence: `target/agent-ready-evidence/`. CI retains its own artifact; local
ephemeral paths are not a claim that evidence is committed or remotely available.

## Review and limitations

Two independent inherited-model reviewers checked standards, requirements and
adversarial failure paths. Accepted findings: SIGTERM cleanup, helper grace period,
existing desktop GUI ownership guard, unlocated warnings and explicit proof mapping.
The runner now records cancellation and stops its process group, allowing 30 seconds
for helpers to shut down owned detached children. Native QA refuses an occupied app ID.

No product UI changed. Inspected the native light/dark file-diff renders emitted by
the suite. The occupied desktop app-ID guard was exercised on a private D-Bus
session and refused native QA before launching any application process.
Real compositor interactions, paid-provider sessions, IME and the broader manual
accessibility matrix are outside automated proof. The evaluation bank is supplied
without trial results; repeated comparative agent runs are future measurement work.
