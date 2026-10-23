# Plan: Oversized terminal scroll counts

## Context and constitution

Use Rust 2021 with the pinned toolchain and existing signaltty-term test module.
Read docs/05, docs/17, docs/19 and the rendered-read requirements in spec 018.
The existing public TerminalBackend and HeadlessBackend::rendered interfaces
provide the regression seam. No new dependencies, state or IPC methods are needed.

## Diagnosis and implementation

The numeric parser already saturates each individual count, but the scroll-budget
sum uses ordinary addition. Two commands overflow that sum; a one-column pane
also overflows when printable text contributes another possible row. Line-motion
controls can increment the saturated budget too. Independent vt100 parsing accepts
these inputs. Saturate every addition to the scroll budget, preserving the existing
conservative upper bound. No refactor is required.

## Verification

Run the public-interface regressions red before changing production code. Assert
literal expected text, independently confirmed by vt100, and continued processing
after each oversized input. Run all signaltty-term tests, independent Standards
and Spec reviews, then stage final source and run scripts/verify.sh full. Check
fresh delivery proof before publishing. Merge only after current PR CI and review
findings are settled. No GUI visuals change; native renders still receive review
as part of the full verification procedure.
