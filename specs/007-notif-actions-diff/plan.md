# Implementation Plan: Notification Actions + Worktree Diff

**Branch**: `007-notif-actions-diff` | **Date**: 2026-09-29 | **Spec**: `specs/007-notif-actions-diff/spec.md`

## Summary

Two closable gaps, no new architecture: a second notification action
reusing `pane.mark_seen`, and a `workspace.diff` method exposing
`git diff --numstat` as data (files, per-dir groups, totals) for CLI,
agents, and the future GUI badge.

## Technical Context

**Language/Version**: Rust 1.85, edition 2021
**Primary Dependencies**: `notify-rust` (already), `git` CLI (already
shelled by `git.rs`)
**Storage**: N/A
**Testing**: notif pure-mapping unit test; `git_diff` unit test on a
fixture repo under `/tmp`; server integration (diff numbers, non-repo
error, handle-or-id); CLI human/JSON test
**Target Platform**: Linux
**Performance Goals**: diff runs on demand only (no polling, no snapshot
bloat); numstat on typical repos is milliseconds
**Constraints**: typed params (`WorkspaceId` reuse); codes from proto (no
new codes — `BAD_PARAMS` for non-repo); GUI diff rendering deferred
**Scale/Scope**: 1 GUI event variant + 1 action, 1 git fn + 1 method + CLI

## Constitution Check

- I: spec → code with tasks inline (two slices, plan doubles as tasks
  given size — tasks.md still written per Principle I).
- V: `workspace.diff` decodes `WorkspaceId`; resolver reuse; result is
  data, never rendered server-side.
- VII: no turn snapshots, no GUI badge, no new events/codes.

## Project Structure

```text
crates/signaltty-gui/src/notif.rs    # mark-read action + action_event() pure seam
crates/signaltty-gui/src/app.rs      # UiEvent::MarkSeen arm
crates/signaltty-gui/src/actor.rs    # UiEvent::MarkSeen variant
crates/signaltty-server/src/git.rs   # git_diff()
crates/signaltty-server/src/router.rs# h_workspace_diff
crates/signaltty-proto/src/lib.rs    # WORKSPACE_DIFF (method 35)
crates/signaltty-server/tests/integration.rs
crates/signaltty-cli/src/main.rs     # workspace diff
crates/signaltty-cli/tests/cli.rs
docs/08-ipc.md + docs/adr/0011-diff-and-deferrals.md
```

## Design

### Notification action

```rust
// notif.rs — pure seam, headless-tested
pub fn action_event(action: &str, pane_id: &str) -> Option<UiEvent>
// "focus" | "default" → FocusPane, "mark-read" → MarkSeen, else None
```

`notify_attention` registers both actions; the wait thread maps through
`action_event`. `App::on_event` handles `MarkSeen` with
`pane.mark_seen` + workspace refresh (same path as terminal focus-clear).

### workspace.diff

```rust
// git.rs
pub struct DiffFile { path: String, added: u64, removed: u64, untracked: bool, binary: bool }
pub struct DiffDir { dir: String, added: u64, removed: u64 }
pub struct WorktreeDiff { branch: Option<String>, files: Vec<DiffFile>, dirs: Vec<DiffDir>, added: u64, removed: u64 }
pub fn git_diff(cwd: &str) -> Option<WorktreeDiff>  // None when not a repo
```

Parse `git -C cwd diff --numstat HEAD -z`? `-z` complicates; paths with
spaces/newlines are edge — use `--numstat` plain lines split: `<added>\t<removed>\t<path>`, binary = `- - path`. Renames (`R100 old new`?) — numstat shows `{old => new}` single path; take as-is (honest, documented). Untracked via `git status --porcelain=v1 --untracked-files=normal` lines starting `?? `. Dir = parent or `.`; rollup sums tracked counts only. Sorted by path for determinism.

Router `h_workspace_diff`: resolve handle-or-id → `git_diff(&ws.cwd)` →
None maps to `BAD_PARAMS("not a git repo: ...")`, else `{workspace_id,
branch, files, dirs, added, removed}`.

CLI `workspace diff ID`: human prints `+A -R path` per file (untracked:
`?? path`), `dir/ +A -R` per dir, totals line; `--json` raw.
