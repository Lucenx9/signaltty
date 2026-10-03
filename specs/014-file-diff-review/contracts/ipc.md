# Selected-file IPC contract

## workspace.file_diff

Request params: `{ "workspace_id": "id-or-handle", "path": "src/main.rs" }`.
Unknown fields are ignored, mistyped/missing fields return `BAD_PARAMS`.
Paths are literal UTF-8 names relative to the Git checkout root, matching `workspace.diff`. Absolute, empty, NUL, current/parent components and Git-private paths are invalid. Glob/pathspec-looking filenames select one exact file. Current tracked files, deletions and unignored untracked files are supported; arbitrary nonmembers are rejected.

Result is `{ "workspace_id": "canonical-id", "path": "src/main.rs", "untracked": false, "content": { "kind": "text", "hunks": [...], "truncated": false, "notice": null } }`.

Hunk: `{ "old_start": 1, "old_count": 1, "new_start": 1, "new_count": 1, "heading": "@@ -1 +1 @@", "lines": [ { "kind": "removed", "text": "old", "old_line": 1, "new_line": null }, { "kind": "added", "text": "new", "old_line": null, "new_line": 1 } ] }`.

Other content cases are `{ "kind": "binary" }`, `{ "kind": "unchanged" }`, or `{ "kind": "unavailable", "reason": "..." }`. The reason is literal display text. Metadata/empty text may have no hunks and an explanatory notice.

Comparison is the current worktree against HEAD, including staged and unstaged edits; before the first commit use an empty tree. Fixed three-line context, no renames, no external diff or textconv, no color. Untracked text is all additions and does not change summary totals. No events, mutation, persistence or terminal operations.

The application does not edit/stage files or mutate Store. Tracked comparisons retain Git's canonical clean/process conversions from trusted local configuration, whose helper programs may have their own effects. Their lifetime is bounded with the Git process group. Inherited `GIT_DIFF_OPTS` cannot override the fixed context.

Limits are 512 KiB preview input/output, 10,000 preview lines, bounded stderr and eight seconds total server time. Truncation is explicit and retains complete lines only; unsupported/oversized output can yield an unavailable notice. Timeout is `TIMEOUT`, I/O failure `IO_ERROR`, invalid/nonrepo path `BAD_PARAMS`, missing workspace `NO_SUCH_WORKSPACE`. The GUI uses a separate read connection and a ten-second deadline.
