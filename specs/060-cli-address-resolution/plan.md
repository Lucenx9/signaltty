# Plan: CLI pane identity and working directories

Date: 2026-10-14. Specification: [spec.md](spec.md).

## Reproduction and cause

Named attach ignores `pane.closed` and `pane.exited` because event payloads
contain opaque IDs while the CLI compares the original name. After name reuse,
its later input requests also resolve to the replacement pane.

Explicit relative cwd travels unchanged over IPC. The server and PTY launcher
resolve it against the server's directory. Sending an absolute path fixes the
same `pwd` fixture without changing the server.

## Implementation

1. Resolve the attach address through `pane.get` before subscribing and attaching.
   Retain the returned ID for all later operations and event comparisons.
2. Share the existing task CLI absolute-path resolver with `new`, `pane spawn`,
   and `pane split`. Apply it only to explicit cwd.
3. Add real CLI regressions in `crates/signaltty-cli/tests/cli.rs`. A Python
   standard-library PTY helper supplies the attach's controlling terminal.
4. Document the caller-relative cwd semantics in `docs/08-ipc.md`.

## Constitution check

Rust 2021 and the existing Linux test environment suffice. No dependency,
schema, persistence, GUI, or architecture change is needed. Test-first fixes
remain in the CLI. Existing task tests cover the shared path resolver.

## Verification

Run the two targeted regression tests, then the complete shared verification
command. Inspect its native renders and record the evidence directory.
