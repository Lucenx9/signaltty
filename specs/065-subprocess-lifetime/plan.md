# Implementation plan: Shared subprocess lifetime

**Branch**: `t3/improve-codebase-architecture` | **Date**: 2026-10-15 | **Spec**: [spec.md](spec.md)

## Summary

Create one private server module for command lifetime. File diffs, turn snapshots, worktrees and task commands prepare their own command and interpret its result. The shared module owns null stdin, piped output, process groups, cancellation, output capture and direct-child reaping.

## Technical context

Rust edition 2021, minimum 1.92, development 1.94. Tokio process/io/time and existing nix process signals. Linux only. No dependencies, persistent data or IPC schema changes. Preserve concurrent output capture and existing operation deadlines and caps.

## Constitution check

Spec and clarification precede implementation. Existing module-level and real IPC tests are the agreed verification seams. Add a real Git-filter regression before migration, then executable tests through the new internal module. No core/GTK/PTY changes. Record ownership in ADR-0039. Full verification and fresh delivery evidence are required. All principles satisfied before design; recheck after implementation.

## Grounding

Selected-file reads own an eight-second outer deadline and byte/diagnostic limits. Their Git runner kills a process group on cancellation, drains both pipes, then interprets limits. Turn captures copy the index and run Git commands with a twenty-second per-command deadline, but only kill the direct child. Worktrees have a separate group guard and concurrent readers. Task commands import file_diff's guard and disarm its public field. See research.md for alternatives and synthesis.

## Caller usage and sketch

Callers prepare tokio::process::Command including arguments, cwd and environment. A single async subprocess::run call accepts that command, an optional deadline and stdout/stderr capture limits. It returns std::process::Output or typed spawn, read, timeout and stream-overflow errors. Callers map those outcomes to their existing response codes and text.

The module owns one Child. Drain stdout and stderr concurrently before polling Child::wait. Until then, the unreaped leader pins its process-group identifier. Once waiting completes, return without an intervening await or group signal. On errors kill the group and await the direct child. On dropped futures kill synchronously; Tokio's kill-on-drop/orphan reaper owns eventual direct-child reaping.

This avoids a detached execution supervisor, cancellation channels or generic command adapters. Cancellation is tied directly to the future's owned child.

## Project structure

- crates/signaltty-server/src/subprocess.rs: private execution implementation and real executable tests.
- Existing file_diff.rs, turns.rs, worktrees.rs, tasks.rs: command policies and result mapping only.
- docs/adr/0039-subprocess-lifetime.md and docs/11-crates.md: ownership and constraints.

## Risks and verification

Do not poll wait concurrently with output capture: a reaped leader could release its PID while descendants retain pipes. Test deadline and cancellation both while the leader runs and after it exits with open descendant pipes. Test success, nonzero exit, missing executable, simultaneous streams and each existing preview limit. Retain file-diff normalization/private-index/merge recovery tests. No new cap for currently uncapped paths.
