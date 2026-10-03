# ADR-0016: Bounded selected-file diffs and native review navigation

**Status**: Accepted, 2026-10-03. **Spec**: [014-file-diff-review](../../specs/014-file-diff-review/spec.md).

## Context

Working Tree Changes reports filenames and counts against HEAD. Reviewing an
agent's edits requires reading the patch. Adding every patch to the summary
would make a count request depend on repository-wide content size. A raw patch
response would make the GUI parse Git hunk syntax to display line numbers.

## Decision

- Add the read-only `workspace.file_diff` method for one literal root-relative
  filename. The server owns Git comparison, path membership, safe file reads,
  classification, subprocess lifetime and limits.
- Return typed hunks and numbered lines from a pure core parser. Binary,
  unchanged and unavailable content have explicit variants. Complete-line
  truncation is visible; it never implies a complete patch.
- Keep the existing worktree-against-HEAD comparison, including staged and
  unstaged edits. Before the first commit, compare against the empty tree.
  Untracked text is displayed as additions without changing summary totals.
- Preserve Git's canonical clean/process conversions from trusted local
  configuration. Signaltty issues no edit/stage/Store mutation; configured Git
  helpers may have their own effects, as with the existing summary. Disabling
  them would redefine normalized HEAD comparisons. Their process group remains
  bounded. External diff/textconv presentation helpers are disabled.
- Bound preview bytes to 512 KiB, lines to 10,000, total server time to eight
  seconds, and the GUI read deadline to ten seconds. Dedicated read connections
  keep ordinary controls and PTY subscriptions available.
- Use one native dialog with an intact file list and a full-width reader.
  Activating a row reads that file. Back restores row focus. A noneditable
  TextView provides selection, copy, keyboard scrolling and long-line scrolling.
- Selection, Back, refresh and closure invalidate older replies. Patch reads
  occur only on explicit activation or refresh, with no persisted selection or
  background polling.

## Consequences

The same parser defines line numbering for server and GUI clients. Fixed
comparison and preview policies keep the method small. Native navigation gives
the code reader usable width without a separate narrow-window layout. Git state,
Store state and terminal widgets remain untouched by the application. Syntax highlighting,
side-by-side rendering, staging and the per-turn snapshots deferred by ADR-0011
remain outside this increment.
