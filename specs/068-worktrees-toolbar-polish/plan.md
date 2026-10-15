# Implementation plan

## Design
Apply frontend-design, emil-design-eng, apple-design and karpathy-guidelines. Existing canvas/view/text/border/accent roles remain; Signal dark canvas #090909 and view #111113 are unchanged. Use native system sans and existing spacing. Layout remains leading New, centered Worktrees, trailing refresh/close. Remove suggested-action fill and use the native flat header style. Existing max-width 480sp behavior hides the label; no new breakpoint or widget abstraction.

Review against brief: this is a task window, so repeated Worktree text and dominant blue creation chrome add noise. A short context-dependent label fits the T3 Code compact toolbar direction while preserving native semantics. Do not replace controls or move shell identity. GNOME header-bar guidance supports a few flat contextual controls: https://developer.gnome.org/hig/patterns/containers/header-bars.html . T3 Code reference50647de0143e68fde051eb1a0ccf4fe63f9947b2, Sidebar.tsx compact creation controls, inspected for this batch.

## Sequence
Extend existing native integration test first: label New, untruncated layout, flat control and complete accessible label; prove behavioral red. Change existing ButtonContent label and style plus restored breakpoint expectations. Verify narrow/large-text and native create/open/remove workflow, capture light/dark. Independent Spec/Standards review and durable evidence precede frozen source. Run full and fresh delivery gate, publish/link, settle latest CI/reviews and merge.

## Technical context and constitution
Rust 1.94 development/minimum 1.92; GTK4/libadwaita; existing workspace_dialogs/app_tests only. No model/persistence/IPC/contract change, no new dependency or load-bearing choice. All constitution principles satisfied. Native allocation needs display tests. Xvfb does not certify real desktop accessibility.
