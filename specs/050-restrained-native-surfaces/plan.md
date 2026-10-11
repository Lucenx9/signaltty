# Implementation Plan

Use explicit application CSS classes on the six native dialogs. Scope button/list/control treatment to those classes; keep default font size, hit targets and native response semantics. Give surfaces 6–10px radii, quiet 1px semantic borders, no raised list/card shadows. Reduce preference selection from double ring to a clear single border with independent keyboard outline. Selected palette results use a neutral fill and leading marker like the main sidebar. Group headings and separators retain hierarchy.

Preserve native close button, switch, text input focus, alert sizing and bottom-sheet behavior; avoid brittle reimplementation of toolkit internals. Inspect exported GTK widget CSS names before selecting dialog-host nodes. Follow installed 1.7 API behavior and validate against actual renders, not only latest documentation.

Constitution: spec/plan/tasks before edits. Styling-only changes add no redundant structural tests; existing full native suite supplies behavior regression gate. Inspect screenshots for normal/hover/focus/disabled state, light/dark and narrow/high-contrast. Independent Standards/Spec review before submission. No dependencies, model/API changes or load-bearing ADR choice.
