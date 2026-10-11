# Implementation plan

Branch `gui/reliable-workspace-interactions`, base `4eadcc0855c281c5b608053e68d166c7a3d73d91`, 2026-10-12. Spec: [spec.md](spec.md).

## Summary and technical context

Rust 2021, minimum 1.92, development 1.94; existing gtk4/libadwaita/VTE. GUI-only changes. App owns IPC, navigation guards and snapshots. New Workspace owns editable form/pending/failure state. Server continues to own session lifetime. No dependencies or external protocol changes.

## Constitution check

Spec, plan, tasks precede source edits. Write behavioral regressions first using existing fake IPC and isolated GTK seams. Preserve structural reconcile and VTE identities. Run full verification and inspect native light/dark/narrow/enlarged/high-contrast scenes. Independent code-review standards/spec reviewers required by docs17. No new load-bearing architecture choice requiring ADR. All gates pass at design stage.

## Grounding and type sketch

Current tab close-page removes local page before server reply; pane close directly sends IPC. Render reconciliation also calls close-page for already-untracked pages, which must bypass user confirmation. New Workspace is an AlertDialog whose responses dismiss it before async work, with validation booleans only. App currently returns on spawn failure without refreshing the partial workspace. Header tools hide at 760sp and remain in menus.

- App: `request_close_tab(page)` and `close_tab_async(id)` delay local removal until successful refresh. Missing/untracked pages from reconciliation finish directly. `close_pane(id)` checks actual `LiveState::Live` and `RestoreState::Live`; one close-target confirmation at a time. Both paths name stable targets and retain widgets on cancellation/failure. Confirm never targets the currently selected replacement.
- New Workspace: native `AdwDialog` with ToolbarView header and Cancel/Create buttons, initial folder focus, default Create and Esc/native close. A small `NewWorkspaceDialog` owns pending controls and result feedback. Callback `Fn(NewWorkspaceRequest, Rc<NewWorkspaceDialog>)`; request carries `existing_workspace_id: Option<String>`. `finish(Result<(), CreationError>)` unlocks retry or closes success; `CreationError` carries the created workspace id only after successful workspace.create.
- App: `create_workspace_async(request, navigation) -> Result<(), CreationError>`, creates once or retries spawn by stable id, refreshes partial success and respects existing navigation generation. Pending form disables editing and can_close; no fake request cancellation.
- Validation: field-local wrapped labels, semantic error style, accessible descriptions, valid input clears errors. Empty custom command and malformed quoting have distinct explanations.
- Compact controls: labeled sidebar footer Board and Changes, visible only below 760sp; Changes bound to the existing split show-sidebar property. On compact activation hide the left overlay to reveal requested content. Wide controls remain existing header entries.

## Visual plan

Use existing semantic foreground/error/accent tokens and system font. Keep 6px native control corners from spec051. Native action dialog header replaces oversized alert form; it follows native content size within a 440px clamp, preserving adaptive scrolling and resizing on font changes. Automatic naming controls appear only for supported commands; errors wrap beside related rows; compact footer reuses existing icon+label controls. No new animation or invented color palette. Error focus is distinct from destructive styling.

## Files and verification

Source: `new_workspace.rs`, `app.rs`; tests: `new_workspace.rs` and `app_tests.rs`. Docs: docs06, project verification feature map, selected docs/qa evidence. Regression cases cover safe cancellation, original target, exited direct close, failure retention, invalid inputs, pending duplicate prevention, workspace-create failure and command retry without duplication, compact activation. Existing full verification includes all new ignored tests automatically.

## Complexity tracking

No deviations. Reuse native dialog/buttons and current actor/test channels; do not add a general workflow engine or server API.
