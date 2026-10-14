# Plan: CLI output gap detection

## Context and constitution

Use Rust 2021 and existing CLI output trimming. The server owns the pane. This fixes an existing delivery contract from ADR-0018 without introducing an architectural decision. Read docs/08, docs/17, docs/19 and the canonical verification skill. Spec and clarification precede implementation; tests precede the fix.

## Implementation

Make the existing unseen_output function return Result. Reject a chunk whose start is greater than the covered cursor before updating the cursor. Propagate the error through attach_loop; run already restores raw mode after this returns. Retain overlap trimming and legacy no-offset behavior.

## Verification

Use the existing real CLI controlling-terminal seam. A scripted Unix-socket peer provides deterministic snapshot and output offsets, avoiding a dependence on producing real overload. Assert exit status, diagnostic, omitted corrupt output, canonical terminal restoration, and contiguous/overlap output. Keep existing unit assertions for legacy offsets. Run scripts/verify.sh full with staged final source and the feature delivery gate. Review standards and spec independently before publishing.
