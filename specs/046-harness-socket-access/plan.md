# Implementation Plan: harness socket access

**Branch**: agent/harness-socket-access | **Date**: 2026-10-11 | **Spec**: [spec.md](spec.md)

## Summary

Add a newline JSON-RPC stdio MCP bridge in the existing CLI. The harness owns
this process outside its shell sandbox. It connects only to explicit session
context and uses existing typed server APIs; no new IPC methods or shell proxy.
The shared integration installer manages provider registrations alongside hooks.

## Technical Context

Rust 1.94 development/minimum 1.92, edition 2021, existing tokio/serde_json clients.
Use toml_edit 0.22 (already locked transitively) for comment-preserving Codex config.
JSON provider entries use existing bounded read, locks and atomic writes.
Linux host; four automatic harness registrations, one stdio bridge binary.
Input capped at 1 MiB, tool execution capped at 60 seconds. Fresh socket per tool
call prevents stream contamination and avoids retrying mutations.

## Constitution Check

Spec/clarify complete, existing seams agreed by project constitution. TDD at CLI
process/real-server and integration configuration boundaries. Core/proto remain
pure. No Store changes. ADR records the new integration boundary. Full verification
required. No unjustified deviations; post-design gates pass.

## Project Structure

- crates/signaltty-cli/src/mcp.rs: protocol and tool routing.
- crates/signaltty-cli/src/main.rs: command dispatch.
- crates/signaltty-cli/src/client.rs: permission-denied connection guidance.
- crates/signaltty-integration/src/mcp.rs: owned provider registrations.
- crates/signaltty-integration/src/lib.rs: shared installer/root/report integration.
- crates/signaltty-server/tests/mcp_bridge.rs: isolated real server/CLI tests.
- crates/signaltty-integration/tests/configuration.rs: provider preservation tests.
- scripts/qa-harness-socket.py: native sandbox reproduction and stdio bridge proof.

## Design decisions

Registrations store the executable and stdio args, never a socket/pane value.
Codex forwards SIGNALTTY_SOCKET/PANE with env_vars. JSON providers inherit session
environment. Explicit provider trust/disabled settings remain untouched.
Owned entries are identified by the signaltty executable basename and exact
`mcp serve --managed-registration` args (the marker grants no privileges); foreign collisions fail rather than being overwritten.
