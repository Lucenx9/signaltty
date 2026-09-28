# ADR-0004 — GUI on GTK4/libadwaita + VTE, core toolkit-free

- Status: accepted
- Date: 2026-09-28

Context: need a native Wayland-first Linux GUI with a terminal
renderer. GTK4/rs has mature bindings plus the VTE widget (a11y, IME,
selection free); Qt6/QML via cxx-qt is early-stage (QtCore/QtGui/QML
only, observed link-flag fragility vs new Qt, no terminal widget —
we would hand-roll rendering).

Decision: Phase 3 GUI = relm4 + gtk4 + libadwaita + `vte4` fed as an
external renderer from the server stream. Core/server/CLI crates stay
toolkit-free so a future Qt or custom-GPU frontend is possible.

Consequences: VTE owns no PTY here (server does); a Phase 3 spike
must validate feed/input mapping early. GTK system deps gate only the
GUI crate, never Phases 1–2.
