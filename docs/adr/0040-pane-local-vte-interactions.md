# ADR-0040: pane-local VTE input and screen reconciliation

- Status: accepted
- Date: 2026-10-15

## Context

The GUI feeds server output into long-lived VTE widgets. The generated `vte4` input callback converts a native string to Rust text before the application sees its explicit byte count. Ctrl+Space therefore becomes an empty message. Native VTE 0.80.1 also loses bytes after an internal NUL when a realized widget's signal machinery copies the string. Reading the reported count from that shortened allocation is unsafe.

Screen recovery currently clears local scrollback. Merely keeping history during reset archives the old visible screen again. The GUI must retain known local history and keep the IPC actor's existing offset-based overlap handling.

## Decision

Keep terminal interactions inside `PaneWidget`. Use a narrow native signal adapter that reads C string bytes without UTF-8 conversion, includes its terminating NUL when the declared count requires it, and rejects counts beyond available storage. It forwards accepted bytes through the existing `pane.input` path and live guard. It does not infer missing bytes or change VTE's global signal registration.

Keep copy, paste and bounded text zoom on a terminal-local action group shared by its shortcuts and native context menu. VTE continues to implement selection and bracketed paste. Zoom is runtime state for that pane. Window commands retain the application action registry.

Read clipboard text asynchronously with cancellation tied to the live pane.
Exiting invalidates pending reads before the pane can resume. Deliver accepted
text through VTE's paste path so terminal paste modes still apply.
Multiple requested pastes remain independent: starting another read does not
cancel an earlier command in the same live process.

Match terminal chords against the active layout's unshifted key before
falling back to shifted aliases. This avoids treating Italian Shift+0 as
the US increase-size alias and lets Italian Shift+plus adjust text size.

Reconcile snapshots with one ordered VTE feed. Leave a stale alternate screen, reset origin and margins, erase each visible row in place, perform DECSTR, then append the unchanged canonical snapshot. On VTE 0.80.1, DECSTR calls the emulator's soft reset without clearing history. Per-row erasure prevents old visible rows from entering history. The actor remains the only owner of output offsets and overlap trimming.

## Consequences

No PTY, history journal, persistent setting or new IPC enters the GUI. Existing widgets remain mounted. Native regression checks cover the byte boundary, duplicate-free history, paste modes, terminal controls and scale.

An embedded-NUL payload already shortened by VTE's native string signal cannot be recovered here. It is rejected. The server snapshot still lacks a saved main grid and explicit alternate-grid identity, so reconnect restores the current grid without promising exact later restoration of a TUI's saved main screen. Recovery of complete history after closing the GUI remains outside this change.

The adapter's unsafe operations must follow the C string allocation bounds. A raw pointer plus the original byte count alone is not an acceptable implementation. Replacing VTE, adding a second PTY or extending the attach protocol would expand ownership and scope without solving the observed keyboard interaction more directly.
