# Implementation Plan: Calm workspace shell

## Technical context
Rust 2021, GTK4/libadwaita, existing resource stylesheet. Presentation only;
no new dependencies, IPC, persistence or core types. Existing ADRs apply.

## Constitution check
Spec and clarified assumptions precede implementation. Write a native display
regression for empty-state actions and narrow geometry before changing code.
Retain the existing shell/navigation/approval tests for structural invariants.
Full verification and light/dark evidence are mandatory.

## Design
Follow research.md and exploration.html. Edit style.css in its existing sections:
quiet progress badges, a neutral inset selection marker, flat header tools with
explicit checked/pressed states, one terminal shadow across focus/attention rules.
Use StatusPage copy and GTK-formatted shortcut labels below existing primary buttons in app.rs.
No custom widget, motion, font, bitmap asset or navigation state.

## Verification
Extend existing native scenes with empty-state capture at desktop and 360px,
light/dark and Sans 18/high contrast. Assert action names and shortcut accelerators,
then exercise keyboard/button activation through existing action tests. Compare
populated screenshots to baseline. Run scripts/verify.sh full and ask a second
reviewer for docs/17 visual rubric review. Save selected evidence and proof limits.
