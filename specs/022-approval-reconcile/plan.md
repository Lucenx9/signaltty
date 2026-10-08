# Implementation plan: Approval choice reconciliation

**Branch**: `gui/approval-reconcile-quality` | **Date**: 2026-10-08 | **Spec**: [spec.md](spec.md)

## Summary

The cache currently remembers only a decision ID. Reuse the existing typed Decision snapshot and rebuild buttons when its ID, answerability or options change. Prompt-only updates retain buttons. No new helper or abstraction is needed.

## Technical context

Rust 2021, minimum 1.92; existing GTK4 and VTE widgets. No dependencies, storage, protocol or core logic changes. Native test seam in terminal.rs can inspect the actual buttons, emitted actions and VTE identity.

## Constitution check

Requirements precede code. Native red-green regression covers the GTK reconciliation defect. Existing light/dark acceptance and full verification remain mandatory. No ADR or constitution deviation is needed.

## Project structure

`crates/signaltty-gui/src/terminal.rs` owns the cache, reconciliation and regression. `docs/06-gui-toolkit.md` records the corrected existing behavior. Selected evidence lives in `docs/qa/`.

## Verification

Run the same native regression before and after, with answerability toggles, changed options and prompt-only updates. Inspect light/dark renders and run scripts/verify.sh full.
