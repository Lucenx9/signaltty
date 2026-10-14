# Execution contract

Preferences exposes Run agents without confirmations, with a description naming Codex, Claude and Antigravity and command/file execution. It saves immediately and applies to future installed-command GUI workspace launches.

Native prefixes: Codex `--dangerously-bypass-approvals-and-sandbox`; Claude `--dangerously-skip-permissions`; agy `--dangerously-skip-permissions --mode accept-edits`.

The standard resume shapes are `codex resume ID`, `claude --resume ID`, `agy --conversation ID`. Matching executable basenames are required, including explicit matching paths. Only those minimal shapes inherit; any explicit resume options win. The generated prefix is inserted after the executable and retained exactly once.

`task.start` with agent and no argv inherits that same provider prefix only when parent_pane_id identifies its automatic native parent. Explicit argv is never modified. Unknown/missing parents and different providers retain current default launch behavior. No new params or error codes.
