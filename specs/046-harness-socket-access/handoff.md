# Handoff

Branch/worktree and base/current commit:
agent/harness-socket-access at
/home/simone/.t3/worktrees/signaltty/agent-harness-socket-access;
base 79e7f7e; functional implementation c501ee9; CLI wording/docs 2bfe5ee.

Accepted requirements and decisions:
Real workspace access despite Codex shell seccomp; stdio MCP outside the shell
sandbox, no network/approval bypass or identity spoofing. Automatic setup for
Codex/Claude/Cursor/OpenCode, audit/manual paths for other bundled harnesses.
Existing hosting context only; native approvals and lifecycle reports excluded.

Completed requirements, with verification command and artifact path:
FR-001 through FR-008 implemented. `scripts/verify.sh full` passed with 34 GTK
tests at target/verification/full-48dkax7b; native Codex proof JSON and logs in
that directory. See verification.md for narrow subsequent CLI validation and
Gemini 3.8 Flash High (Antigravity) / Standards review results.

Uncommitted changes and file ownership:
No implementation changes remain. Root integrated all files; both reviewers
were read-only. Delivery documentation is committed before publication.

Remaining failures or unverified behavior:
None in repository gates. No authenticated provider tool turn; other-harness
runtime boundaries vary and whole-process sandboxes need host deployment.
Native GUI accessibility/compositor behavior was not changed or re-certified.

Next concrete step:
Push this branch, open the PR against main and link it in T3. Merge requires a
new instruction; the user authorized publishing this PR.
