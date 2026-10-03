# Implementation plan: Per-file diff review

**Feature**: `014-file-diff-review` | **Date**: 2026-10-03 | **Spec**: [spec.md](spec.md)

## Summary

Extend Working Tree Changes with a native full-width List → Reader → Back flow. Read one selected file on demand through `workspace.file_diff`, return shared typed hunks, and show literal selectable monospace text with old/new line numbers. Git remains the comparison authority. Existing summary counts and terminal widgets remain unchanged.

## Technical context

- Rust 1.85 / edition 2021 workspace; GTK4 0.11, libadwaita 0.9, Tokio, serde, existing nix dependencies.
- No new runtime dependencies or persisted state. Linux native desktop, local Git repositories.
- Tests: core in-module behavior, testkit real-socket/temporary-repository integration, actor concurrency, ignored GTK display tests.
- Bounds: 512 KiB retained patch/file bytes, 10,000 preview lines, bounded stderr, 8-second total server request and 10-second GUI deadline. Partial text contains complete lines and is visibly truncated. Oversized/unrenderable cases may return an explicit unavailable notice.
- One reader at a time in a dialog; no automatic polling. Dedicated IPC read connections keep control and subscriptions responsive.

## Constitution check

Pre-design and post-design gates pass. Spec and clarification scan precede plan/tasks/code. Domain parser is pure core; Git/OS/async remain server-owned; GTK stays GUI-only. Typed params and canonical IPC constants/docs/tests are required. Read operations emit no mutations. TDD at existing documented public seams and workspace gates are required. Native light/dark/narrow captures prove rendering. ADR-0016 records the surviving payload/navigation decision. No exceptions.

## Grounding and design

`App::action_show_changes` opens `workspace_dialogs::changes`, which calls the background actor for `workspace.diff`. Router copies cwd under the Store read lock and releases it before Git. The summary reports combined staged/unstaged changes against HEAD plus untracked paths. Worktree operations already demonstrate separate IPC connections for expensive Git calls. New detail requests use that isolation with a read-specific deadline, not the mutation timeout wording.

The chosen UI owns both list and reader within one dialog. Native row activation opens the reader; Back restores the row's focus. No automatic first-file read. TextView handles selection, copy, keyboard scrolling and long-line horizontal scrolling. Tags use semantic colors, with signs providing redundant meaning. The dialog owns alive/selection/generation state; selection, Back, refresh and close invalidate old responses.

The server entry `file_diff::read(cwd, path)` owns root resolution, exact literal membership, fixed comparison flags, file safety, bounded subprocesses and result classification. It does not regenerate the full summary for each selected file. Deleted tracked files remain readable. Untracked regular text is synthesized as additions from bounded no-follow reads; links/special files never become external-content reads. Git's external diff/textconv/color/relative/rename settings cannot redefine the payload. Root-relative semantics also apply when workspace cwd is a subdirectory.

Tracked reads use Git directly so `.gitattributes`, CRLF normalization and canonical clean/process conversions match HEAD semantics. Local Git helpers are trusted, as in the existing summary; their own side effects are not an application-owned state mutation. The timeout bounds their process group. Disable presentation helpers and inherited `GIT_DIFF_OPTS`; do not disable canonical conversion or replace it with a second comparison engine. Untracked filesystem failures cross the boundary as `IO_ERROR`; refused preview states use a typed unavailable outcome.

Shared `signaltty_core::diff` contains `FileDiff`, tagged `DiffContent`, `DiffHunk`, `DiffLine`, and `DiffLineKind`. Parsing ignores quoted filename headers and obtains the literal path from the request. The parser tracks hunk ranges and line numbers, distinguishes missing-final-newline markers, and supports complete-line truncation without inventing text. No parser lives in GTK code.

## Project structure

- `crates/signaltty-core/src/diff.rs`, `lib.rs`: pure domain types/parser and tests.
- `crates/signaltty-server/src/file_diff.rs`, `lib.rs`, `router.rs`, `params.rs`: bounded Git/file read and typed async route.
- `crates/signaltty-proto/src/lib.rs`: canonical method/schema entry.
- `crates/signaltty-server/tests/file_diff.rs`: public IPC behavior with real Git fixtures.
- `crates/signaltty-gui/src/changes.rs`, `workspace_dialogs.rs`, `main.rs`, `actor.rs`, `app_tests.rs`, existing CSS resource: native reader, dedicated request and GTK proof.
- `docs/06-gui-toolkit.md`, `docs/08-ipc.md`, `docs/02-data-model.md`, `docs/adr/0016-file-diff-review.md`: durable behavior and boundary decisions.

## Implementation and verification seams

The user authorized the complete feature and its verification. Use the repository-mandated seams: pure-core parser, public IPC, actor requests, and actual GTK controls. Sequence tracked text first, then keyboard/lifecycle, then new/binary/bounded edge behavior. GUI and server ownership can progress concurrently against the fixed contract. Each owner records red before green; no speculative test framework or external tracker publication.

No complexity deviations or unresolved design questions remain.
