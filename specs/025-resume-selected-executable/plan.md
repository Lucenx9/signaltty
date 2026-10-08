# Implementation Plan: Resume Retains Selected Executable Path

**Branch**: `t3/resume-selected-executable` | **Date**: 2026-10-08 | **Spec**: [spec.md](spec.md)

## Summary

Implement a pure helper `resolve_resume_argv(original_argv: &[String], resume_argv: &[String]) -> Vec<String>` in `signaltty-agent`. Wire it into `signaltty-server::router` at the two session identity writers (`h_hook_event`, `h_pane_report_session`) and at resume execution (`h_pane_resume`).

## Design & Architecture

### 1. Pure Helper in `signaltty-agent`

`crates/signaltty-agent/src/resume.rs`:
```rust
pub fn resolve_resume_argv(original_argv: &[String], resume_argv: &[String]) -> Vec<String>
```
Rules:
1. If `resume_argv` is empty, return empty vector.
2. If `resume_argv[0]` contains `/` (path-qualified, e.g. explicit manifest path), return `resume_argv` as-is.
3. If `original_argv` is empty, its first path is not absolute, or the resume program is empty, return `resume_argv` as-is. Legacy relative paths have no reliable launch directory.
4. Extract `original_basename` from `original_argv[0]`. If `original_basename == Some(resume_argv[0])`:
   Replace `argv[0]` with `original_argv[0]`, keeping `resume_argv[1..]`.
5. Otherwise (e.g. promoted shell, wrapper with different basename), return `resume_argv` as-is.

### 2. Integration Seams in `signaltty-server`

- `h_hook_event`: When `adapter.session_identity` updates `p.agent.resume_argv`, resolve via `resolve_resume_argv(&p.argv, &resume_argv)`.
- `h_pane_report_session`: When writing `pane.agent.resume_argv`, resolve via `resolve_resume_argv(&pane.argv, &r.argv)`.
- `h_pane_resume`: When extracting `raw_argv` from `pane.agent.resume_argv`, resolve via `resolve_resume_argv(&pane.argv, &raw_argv)`, spawn with `resolved`, and update `pane.agent.resume_argv = Some(resolved.clone())` only after successful spawn.

## TDD Plan

1. **Red Test Phase**:
   - Write pure unit tests in `crates/signaltty-agent/src/resume.rs` covering:
     - 5 built-in adapters (`codex`, `claude`, `opencode`, `cursor`, `pi`)
     - Relative paths (`./bin/codex`, `sub/bin/claude`)
     - Empty inputs (`[]`, `[""]`)
     - Bare commands (`["codex"]`)
     - Promoted shells (`["/bin/bash"]`)
     - Explicit manifest paths (`["/manifest/bin/codex", ...]`)
     - Suffix flags on original command (`["/opt/codex", "-m", "gpt-4"]`)
     - Differing wrapper basenames (`["/usr/local/bin/my-wrapper"]`)
   - Write real IPC integration regression tests in `crates/signaltty-server/tests/integration.rs`:
     - Sibling test `report_session_builds_resume_retains_selected_executable_without_manifest`
     - Hook-event writer test
     - Stale bare snapshot repair test
   - Run tests and record RED failure output.
2. **Implementation Phase**:
   - Implement `resolve_resume_argv` and export from `signaltty-agent`.
   - Update `h_hook_event`, `h_pane_report_session`, and `h_pane_resume` in `crates/signaltty-server/src/router.rs`.
3. **Green Test Phase**:
   - Run the new unit and integration tests and verify GREEN.
   - Run clippy and fmt on `signaltty-agent` and `signaltty-server`.
4. **Documentation**:
   - Update `docs/07-agents.md` and `docs/08-ipc.md`.

Review follow-up: normalize relative path-qualified argv0 before spawn/split, using launch cwd under the existing Store lock, without canonicalizing symlinks or changing bare PATH commands. Await and reap the owned server via the existing `wait_for_exit` testkit helper before editing old snapshot fixtures.
