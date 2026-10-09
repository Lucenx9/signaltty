# Plan: Header wayfinding
Rust 2021 / GTK4 / libadwaita, current pinned toolchain. Touch app.rs header construction and workspace presentation, native app_tests.rs and docs06 only. Bind separator to context visibility instead of workspace mark. Prefer existing agent summary; fallback to existing tooltip location. Clear visibility with no workspace. Add native regression before implementation, exercise actual App/snapshot boundary, check allocation before introducing sizing changes.

## Constitution check
Spec/plan/tasks precede code; test-first GTK seam is required for actual widget visibility/allocation. Core/proto remain untouched; no ADR is required for restoring existing documented context behavior. Use apple-design, emil-design-eng and diagnosing-bugs; full verification and inspected light/dark captures before PR.

## Verification
Native shell/agent/missing-branch/empty-state fixture. Bound checks at 360px normal and Sans18, native light/dark captures, original workflow and full gate. Grok4.7 direct xAI independent scope review, Sonnet5.5 final diff review. Parent sole writer and builder.
