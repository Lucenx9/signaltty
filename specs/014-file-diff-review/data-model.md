# Transient diff model

These values are read results, not persisted workspace state.

- `FileDiff`: literal root-relative `path`, `untracked`, and `content`.
- `DiffContent`: serde tagged by `kind`, snake_case. `Text { hunks, truncated, notice: Option<String> }`, `Binary`, `Unchanged`, or `Unavailable { reason: String }`.
- `DiffHunk`: `old_start`, `old_count`, `new_start`, `new_count` as u64, `heading` as a literal hunk heading, `lines`.
- `DiffLine`: `kind`, literal `text` without diff prefix/newline, `old_line`, `new_line` as optional u64.
- `DiffLineKind`: `context`, `added`, `removed`, `no_newline`. Context advances both counters, additions new only, removals old only, newline markers neither.

Text with no hunks and a notice describes an empty new file or metadata-only change. Unchanged describes a known tracked file that became clean. Binary has no text payload. Unavailable explains unsupported content or refused preview. A truncated text result preserves complete lines but declared hunk counts may exceed its retained prefix. GUI must label it incomplete.

Dialog state is confined to a single open dialog. An accepted reply must match its alive flag, request generation and literal selected path. Back, refresh, another selection and close invalidate the previous generation. Returning to the list restores selection/focus; no event causes a background patch read.
