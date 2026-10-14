# Implementation plan: Automatic agent permissions

**Branch**: `agents/automatic-permissions` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary

Add a default-off GUI preference; convert supported installed bare launch commands into native automatic commands. Preserve Signaltty's generated automatic prefix during standard resume and default same-provider task starts. Explicit commands retain precedence.

## Technical Context

Rust 2021, pinned Rust 1.94 development toolchain, GTK4/libadwaita GUI. Existing serde JSON preference storage and pane snapshot arguments. No new dependencies or provider config writes. Linux desktop. Pure argument logic tests, existing preference tests, server IPC tests with fixture processes, isolated GTK tests and `scripts/verify.sh full`.
Performance: constant small argv work only, no provider invocation or polling added.

## Constitution Check

Spec/plan/tasks precede code; existing confirmed test seams are reused. Core stays toolkit-free, pure argument transformation stays in signaltty-agent. No new state transitions or IPC methods. Native controls use existing styles. ADR records permission precedence. Default-off preserves existing behavior. No unresolved questions or deviations, before and after design.

## Project Structure

- `crates/signaltty-core/src/theme.rs`: persisted preference parsing.
- `crates/signaltty-agent/src/execution.rs`, `resume.rs`: pure native automatic mode helpers and resume preservation.
- `crates/signaltty-gui/src/preferences.rs`, `app.rs`, `new_workspace.rs`: persistent switch and installed-command launch choice.
- `crates/signaltty-server/src/tasks.rs`: default worker inheritance.
- Existing core, agent, GTK and server test files: regression coverage.
- `docs/07-agents.md`, `06-gui-toolkit.md`, `08-ipc.md`, `adr/0036-automatic-agent-permissions.md`: contracts and limits.

## Verification Strategy

Red-green slices: persisted boolean; managed automatic commands; standard resume; default worker inheritance; native control callback and appearance. Tests use existing public parse/launch/resume/IPC interfaces and native widgets. Full gate plus native light/dark renders; no paid provider prompts are run or claimed.
