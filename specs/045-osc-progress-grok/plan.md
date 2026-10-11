# Implementation Plan: OSC progress and Grok

## Summary
Parse progress in the raw-byte OSC scanner, retain it in the PTY-owned OscScanner,
and feed an explicit progress string to the declarative matcher and explain.
Bundle Grok as a program-scoped generic manifest, reusing Store transitions.

## Technical Context
Rust 2021, minimum 1.92; existing vt100/regex/TOML/portable-pty dependencies.
Native Linux server; no new dependencies or GTK implementation changes.
Test seams: OscScanner::push, ScreenRule::matches/classify, bundled discovery,
real TestServer IPC + PTYs. Constant small evidence per pane; existing 500ms tick.

## Constitution Check
Pass before and after design: spec before code; test-first at existing seams;
core/proto toolkit-free; lifecycle changes via Store; no new IPC method;
ADR-0030 records ephemeral progress and compatibility boundaries. Full verification
and independent Standards/Spec reviews before delivery. No deviations.

## Project Structure
- `crates/signaltty-term/src/osc.rs`: reserve progress, emit progress evidence.
- `crates/signaltty-agent/src/screen.rs`: progress region and explicit input.
- `crates/signaltty-agent/screen/grok.toml`, `src/bundled.rs`: Grok rules/discovery.
- `crates/signaltty-server/src/pty.rs`: read evidence from the PTY-owned scanner.
- `crates/signaltty-server/src/router.rs`: classification and explain inputs.
- `crates/signaltty-server/tests/screen_detection.rs`: real PTY tests.
- `docs/07-agents.md`, `docs/05-terminal-backend.md`, `docs/08-ipc.md`, ADR-0030.

## Implementation strategy
One red/green behavior at a time: reserve progress without notification; expose
region; port Grok; integrate cache and explain; check hooks and reset. Existing
classifier callers gain the new explicit input; do not retain a compatibility wrapper.
