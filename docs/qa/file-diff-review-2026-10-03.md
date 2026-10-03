# Per-file diff review — 2026-10-03

Feature: [014-file-diff-review](../../specs/014-file-diff-review/spec.md).
Reviewed against `edf10ce`; delivered on `main`.

Working Tree Changes now opens a native, selectable, read-only file reader with
numbered context, additions and removals. It compares the current worktree to
HEAD, including staged and unstaged changes. Back preserves the selected row
and list position; refresh, navigation and close invalidate older replies.
New, empty, binary, unchanged, unavailable and incomplete previews have explicit
states. The reader retains terminal widgets.

## Verification

- `cargo build --workspace`, `cargo fmt --check`: pass.
- `cargo clippy --workspace --all-targets`: pass with the existing untouched
  `audit.rs:111` open-options warning.
- `cargo test --workspace`: **248 passed, 0 failed, 15 display tests ignored**.
- The new ignored `file_diff_reader_uses_native_numbered_selectable_controls`
  GTK test passes under isolated Xvfb/D-Bus, exercising actual Enter activation,
  native selection and clipboard copying, Back/refresh, stale replies,
  replacement-dialog isolation and destruction of the closed reader.
- The existing ignored
  `navigation_palette_fast_enter_and_git_dialogs_use_native_controls` test passes.
- Ten real-Git socket tests cover tracked/new/deleted/binary files, unborn HEAD,
  staged plus unstaged edits, literal unusual paths, subdirectory workspaces,
  normalization, validation, refused symlinks/FIFOs, I/O errors and preview limits.
  The actual eight-second deadline stops slow filter descendants while another
  request completes.
- A public IPC regression passes with `GIT_DIFF_OPTS=--unified=0`; the reader
  retains its promised three lines of context.

Independent standards, spec and adversarial reviews found four actionable issues:
sorted-list empty-state removal, duplicate/imprecise truncation notices,
operational errors classified as unavailable, and inherited Git context options.
Each has a regression and a verified correction. The final workspace run also
caught and corrected the protocol test's method count. No open finding remains.

## Visual evidence and bounds

Actual GTK captures were inspected after the final corrections:

| Surface | Evidence |
|---|---|
| Light theme | [image](file-diff-review-2026-10-03/file-diff-light.png) |
| Dark theme | [image](file-diff-review-2026-10-03/file-diff-dark.png) |
| Narrow, 360px | [image](file-diff-review-2026-10-03/file-diff-narrow.png) |
| Incomplete new-file preview | [image](file-diff-review-2026-10-03/file-diff-incomplete.png) |
| Binary notice | [image](file-diff-review-2026-10-03/file-diff-binary.png) |

The GTK test supplies responses at the production actor seam; the separate
real-Git tests prove acquisition and IPC behavior. Addition/removal colors adapt
to both themes, with redundant signs and readable line numbers. Back and Refresh
remain reachable at 360px. Gate logs, red/green context evidence and review
reports are in the [artifact directory](file-diff-review-2026-10-03/).

Previews are bounded to 512 KiB and 10,000 complete lines; oversized content can
be unavailable, and retained partial previews say they are incomplete. The
server deadline is eight seconds; the GUI has an isolated read connection and a
ten-second deadline. Trusted local Git clean/process conversions remain active
for canonical comparisons; external diff and textconv presentation helpers are
disabled. The application performs no staging, file edits, Store transitions or
terminal operations. This feature compares to HEAD and does not capture agent
turns. The other thirteen ignored display tests were not rerun for this increment.
