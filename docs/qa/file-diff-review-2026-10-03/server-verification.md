# Selected-file core/server verification

Owned T003/T004/T005/T010/T011 code is complete. No docs, Git commits or task-status edits made.

Red/green slices observed:
- Public socket tracked staged+unstaged/deletion test first failed UNKNOWN_METHOD workspace.file_diff; green after canonical method, typed params, router and bounded reader.
- Pure numbered-hunk/CRLF/no-final-newline parser first failed `not implemented`; green after parser.
- Pure regular-file→symlink two-section parser first failed Invalid unified diff line; green after between-section header handling.
- Public Git customized indicators/blank-context test first returned unavailable Invalid unified diff line; green after fixed indicator/blank-context config.
- Pure forced-text NUL-content test first returned Text containing NUL; green after binary classification.

Verification:
- `cargo build -p signaltty-server`: pass.
- `cargo test -p signaltty-core diff::tests -- --nocapture`: six parser/builder tests pass.
- `cargo test -p signaltty-server --test file_diff -- --nocapture`: all nine real-socket fixtures pass (8.30s including actual eight-second deadline).
- Added no-mutation/event-sequence assertions in the tracked fixture and tracked line-cap assertions in the limits fixture; both focused reruns pass.
- `cargo clippy -p signaltty-core -p signaltty-proto -p signaltty-server --all-targets`: pass, only existing untouched audit.rs:111 suspicious_open_options warning.
- Owned Rust files formatted with rustfmt; root owns full workspace gates and display evidence.

Coverage: combined staged/unstaged two hunks and deletion, handle resolution/canonical id, unchanged and metadata paths, new/empty/binary/unborn, CRLF normalization and forced binary attrs, explicit literal unusual filenames/pathspec syntax, subdirectory summary/detail root-relative consistency, directory/nonmember/ignored/nonrepo/traversal/mistyped rejection, method/schema, old/new numbering, malformed ranges/encoding, exact complete-line 10k truncation, 512 KiB stdout/untracked caps, symlink parent/leaf no external content, no-follow untracked symlink and FIFO no blocking, no textconv/external diff despite config, deadline stops slow clean-filter descendants while independent server.status succeeds.

Stable implementation: tracked reads use direct bounded targeted Git with fixed no-color/no-ext-diff/no-textconv/no-renames/no-relative/three-line-context/short-submodule settings. Git owns normalization and attributes. Untracked reads use descriptor-relative libc openat O_NOFOLLOW through every component, final O_NONBLOCK and metadata regular-file check, capped read on spawn_blocking. Pure builder/parser live in core, shared serde payload has no GTK. No state mutation/event/persistence. Each Git process has a process-group kill guard plus kill_on_drop; EOF/cap reaps explicitly, total timeout drops and stops the process tree.

Limits: stdout/file retained bytes 512 KiB (oversize unavailable), diff/new text 10,000 complete lines (explicit truncated + notice), stderr 8 KiB (IO_ERROR), total server budget eight seconds. JSON size remains below 16 MiB.

## Standards-review correction

Public unreadable-untracked test runs as uid 1000 with chmod000. Red: response was successful Unavailable; green: response is IO_ERROR. A small internal SnapshotError now distinguishes actual directory/file open, metadata and read errors from refused symlink/special/oversized previews. Missing file remains BAD_PARAMS. Focused unreadable, symlink/FIFO and preview-limit tests pass; owned files formatted.

Git filter research: official tagged diff.c lines 3966–3975 invokes convert_to_git when loading worktree contents; convert.c lines 1348–1378 applies configured clean/process conversion before encoding/CRLF/ident handling. `--no-textconv` suppresses binary presentation filters, rather than ordinary canonical clean filters. Preserving trusted configured normalization is the smallest honest HEAD comparison; blindly disabling clean drivers can show incorrect encrypted/LFS/platform content. Root owns final documented read-only semantics. Sources: https://github.com/git/git/blob/v2.47.3/diff.c#L3966 and https://github.com/git/git/blob/v2.47.3/convert.c#L1348 and https://git-scm.com/docs/git-diff.
