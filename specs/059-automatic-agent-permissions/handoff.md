# Handoff: Automatic agent permissions

Branch/worktree: `agents/automatic-permissions`, `/home/simone/.t3/worktrees/signaltty/orch-idempotent-task-reports`.
Base: `225faa8553cdf564b1aed960038367b2ea683bd2`.
Implementation: `4a69bea05c48e2a925c6ceb32ad32f88e80e46e8`.
Final reviewed source/docs: `e1ddc34605799df4cce870380b61f08aee068341`.

Accepted requirements: one persisted opt-in GUI launch mode for Codex/Claude/Antigravity; retain generated prefix on standard resume and default same-provider Codex/Claude workers. Keep explicit commands and Once/Deny authoritative. Another PC receives the fix by installing the updated build; this PC's provider settings were not modified.

Completed: FR-001 through FR-007. Native boolean, command, GTK and real-PTY fixtures cover the contracts. Red regressions observed before implementing preference parse, native automatic launch, resume and default worker mode. One initial resume fixture incorrectly expected Pane.argv to be rewritten; corrected to the existing documented agent.resume_argv contract and actual process output.

Verification: `scripts/verify.sh full`, clean source commit 4a69bea, 57/57 steps passing, 45 isolated GTK tests, server QA and refresh benchmark. Artifact directory `target/verification/full-butqqo_d`; selected durable proof in `evidence/verification.json` and the light/dark native PNGs. Later edits concern documentation, task status and proof only. Both native screenshots inspected after loading production stylesheet and icons.

Independent Standards review: 0 findings. Spec review: 1 documentation precision finding, fixed in e1ddc34 by limiting default worker inheritance to existing Codex/Claude native adapters. Final Gemini 3.8 Flash High Antigravity review: PASS, 0 findings on e1ddc34, including verification artifact inspection. See `evidence/review.json` for exact delegated task.

Not certified: paid provider turns, live user account trust/authentication, another PC's installed build, native compositor accessibility/IME/folder chooser/notification clicks. No pending request is auto-answered and no live session policy is changed.

Remaining: publish and link the PR, record its URL. No code failures or unresolved reviewer findings.
