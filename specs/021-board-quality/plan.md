# Implementation plan: Task Board quality

**Branch**: `gui/quality-polish-20261008` | **Date**: 2026-10-08 | **Spec**: [spec.md](spec.md)

## Summary

Place the existing five-column board inside a native horizontal scroller. Keep each column's existing vertical scroller. Replace double metadata muting with one semantic foreground treatment and remove header muting in high contrast.

## Technical context

Rust 2021, minimum 1.92, pinned development toolchain. GTK4, libadwaita and existing CSS only. No storage or protocol change. Native Linux desktop, 360px and wide windows. Existing TaskCardView and BoardColumnView remain the data shape. No new abstraction is needed.

## Constitution check

Spec precedes code. Add a failing native GTK regression before implementation. Existing full verification remains the delivery gate. Core and protocol remain toolkit-free. Task classification, event handling and terminal reconciliation stay unchanged. Light and dark renders are inspected. No load-bearing architectural decision or deviation.

## Project structure

- `crates/signaltty-gui/src/board.rs` owns native board construction.
- `crates/signaltty-gui/data/style.css` owns board typography and contrast.
- `crates/signaltty-gui/src/app_tests.rs` owns the native regression and screenshots.
- `docs/06-gui-toolkit.md` records the existing board's corrected presentation.

## Verification

Prove 360px bounds before and after, native horizontal adjustment reaches Done, keyboard focus reveals the last card, and activation delivers its pane. Capture normal light/dark and enlarged high-contrast scenes. Run `scripts/verify.sh full`.
