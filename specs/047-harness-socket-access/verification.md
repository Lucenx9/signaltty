# Verification

Date: 2026-10-11. Base: 79e7f7e. Implementation and review fixes: c501ee9.

## Behavioral proof

- Red: the previous real CLI rejected `mcp` as an unknown subcommand. The new
  Codex registration preservation test initially found no managed MCP entry.
- Green: signaltty-integration configuration suite: 23 cases, plus one unit
  test. This includes renamed binary refresh, provider roots, foreign/disabled
  entries, unrelated settings, malformed/FIFO files, atomic writers and cleanup.
- Real CLI/server MCP suite: four tests pass. Hosting context and server-owned
  worker output, missing/stale context, initialization/JSON errors, rejected
  streams/identity/approval methods and 1 MiB closure are exercised.
- Managed Claude spawn/split lifecycle fixture passes and verifies registration
  exists before launch. Existing provider opt-outs retain their setup behavior.
- Native Codex CLI 0.162.1 isolated probe passes. A workspace-write shell gets
  EPERM both before and after a host-owned MCP bridge identifies the pane and
  starts a worker with observable PTY output. Restricted CLI errors point to the
  MCP tools. `codex mcp get signaltty --json` accepts the generated registration.
  Evidence: target/verification/full-48dkax7b/native-mcp-proof.json;
  script: scripts/qa-harness-socket.py.
- Isolated installed Claude `mcp get signaltty` accepts the generated command;
  its custom config root matches the native config-location probe.

Native probes perform no authenticated model turns. The bridge process boundary
is exercised directly, rather than through a paid Codex session. The harness
matrix in docs/07-agents.md separates these proofs from help/docs-only support;
whole-process sandboxes can enclose local MCP too.

## Standards review

Parallel code-review reviewer found one P1: `decision.answer` could resolve a
native provider approval and violated ADR-0032's boundary. Fixed by excluding
it from discovery and execution. Red test observed it in discovery; green test
checks absence and rejection. Reviewer rechecked f1eb7a0 at c501ee9: **0 remaining
Standards findings**, no baseline smell requiring changes.

## Spec review

User-requested T3 delegated Antigravity **Gemini 3.8 Flash High**
(`gemini-3.8-flash-high`) completed before PR creation. Verdict: **no blocking
findings**, no missing/partial requirements, scope creep or incorrect behavior
found. It noted the authenticated-turn and other-provider proof limits above.
The separate Standards review's narrow approval exclusion is recorded explicitly.

## Repository gate

`scripts/verify.sh full` passed: evidence
`target/verification/full-48dkax7b/summary.json`. Tooling, formatting, locked
metadata/build, architecture, Clippy warning policy, workspace tests, server QA,
all **34 isolated GTK tests** and refresh benchmark passed. Started at c501ee9;
the subsequent CLI cleanup wording change in 2bfe5ee was also compiled by the
workspace tests and checked with `cargo clippy -p signaltty-cli --all-targets
--locked -- -D warnings`, then rebuilt before the final native probe.

Inspected `screenshots/changes-light.png`, `changes-dark.png` and
`details-light.png` from that run: native controls remain readable and fit.
The integration notice's existing GTK test passed in both themes; it asserts
visibility rather than saving notice screenshots. This feature changes no GUI
layout, and these renders do not certify compositor or authenticated-provider
behavior. The first full runs rejected a new question_mark lint; c501ee9 fixed it.

Final native QA output: denied_before=true, denied_after=true,
context_matches=true, worker_output="native MCP worker reached host",
native_config_accepted=true, permission_guidance=true. Its isolated server log
is preserved under `target/verification/full-48dkax7b/harness-socket/`.
