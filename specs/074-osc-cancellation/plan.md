# Plan: Cancelled terminal control strings

## Context and constitution

Use the existing Rust OSC scanner and public `OscScanner::push`/`progress` seam.
Read docs/05, docs/17, docs/19 and ADR-0030. Preserve bounded carry buffers,
notification sanitization and the separate screen parser. No new dependencies,
data model or architectural decision is needed.

## Diagnosis and implementation

The red regression publishes `cancelled\u{18}x` as OSC 9 content and consumes
the following BEL. Probe each split point and other OSC types to distinguish a
missing cancellation rule from carry-buffer or notification-specific defects.
Treat CAN and SUB as aborted strings in the shared end finder. Resume scanning
after the cancellation byte, both in normal parsing and oversized discard mode.

## Verification

Write failing public-interface regressions first. Cover CAN/SUB, notifications,
titles, retained progress, subsequent valid OSC and oversized discard recovery.
Run the terminal suite and independent Standards/Spec reviews against origin/main.
Stage final source and run `scripts/verify.sh full`, inspect native renders and
check delivery proof for this feature. Publish and link one PR, then assess current
CI and comments before the authorized merge. GUI appearance is unchanged.
