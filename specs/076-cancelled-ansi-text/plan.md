# Plan: Keep text after cancelled ANSI sequences

## Context and constitution

Rust 2021, minimum 1.92, existing toolkit-free term sanitizer. Read docs/05, docs/10 and ADR-0002. Match the OSC scanner's cancellation semantics without changing its public API or terminal rendering. No new architectural decision or dependency is needed.

## Diagnosis and implementation

Standalone harnesses calling the production strip_ansi show two failures. Cancelled OSC consumes all following text until a later terminator; cancelled CSI consumes the next ASCII letter as its final byte. A sanitizer-only cause is ruled out because strip_ansi itself fails. A later BEL/ST does not recover the swallowed text. Stop each escape skip at CAN/SUB and consume the cancellation byte, then resume the existing text loop.

## Verification and delivery

Write in-module public-interface regression tests before the correction, including both cancellation bytes, OSC and CSI, later terminators, Unicode and subsequent valid escapes. Verify the notification sanitizer's control removal and limit. Run term tests, independent Standards/Spec reviews against the pinned base, then stage frozen source and run scripts/verify.sh full. Inspect native renders and check fresh delivery proof. Publish a linked PR and merge after current CI and review decisions settle. Real desktop and paid provider checks remain not run.
