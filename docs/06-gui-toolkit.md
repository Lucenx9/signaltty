# 06 — GUI Toolkit Decision: GTK4/libadwaita (over Qt 6/QML)

Evaluated against: Wayland-first, KDE Plasma + GNOME, GPU rendering,
keyboard/IME/clipboard, splits, custom drawing (attention rings),
accessibility, notifications, and Rust binding maturity.

## GTK4 + libadwaita (gtk4-rs + relm4) — RECOMMENDED

- Mature Rust bindings (`gtk4` 0.10/0.11, `libadwaita` 0.8/0.9,
  `vte4` 0.9/0.10), GNOME 50 runtime, GTK 4.24 Wayland improvements
  (fractional scaling, text-input v3.2, ext-background-effect).
- **VTE terminal widget exists** — no other toolkit gives us a native
  terminal renderer with a11y/IME/selection for free.
- Wayland is GTK's primary backend; X11 fallback works; runs fine on
  Plasma (GJ issues are theming-level, not functional).
- Custom drawing for attention rings: `GtkDrawingArea`/CSS borders —
  trivial.
- Desktop notifications: `notify-rust`/`ashpd` integrate regardless of
  toolkit; `.desktop` + StartupWMClass story is standard.
- Relm4 (Elm-style) keeps Rust core independent: core crates stay
  toolkit-free, GUI crate depends on core types only.

## Qt 6 / QML (cxx-qt) — REJECTED for now

- `cxx-qt` is KDAB-maintained but self-described early-stage; supports
  QtCore/QtGui/QML only (no Widgets/KDE Frameworks bindings).
- Real-world fragility confirmed in 2026 reports: link-flag
  incompatibilities against newer Qt (6.11), per-module bridge layout
  constraints (QTBUG-93443 workarounds), C++ toolchain in build.
- **No terminal widget**: we'd hand-roll QML terminal rendering
  (fonts/shaping/IME/a11y) — the single biggest GUI risk, with zero
  reuse.
- KDE integration would be nicer (Kirigami, layer-shell), but not
  worth owning a terminal renderer.

## Decision

Build the Phase 3 GUI on **GTK4 + libadwaita + VTE via gtk4-rs/relm4**,
with all logic in toolkit-independent core crates. Keep the
`TerminalBackend` trait (see [05](05-terminal-backend.md)) so a Qt or
custom-GPU frontend stays possible without touching the server.

Note: this machine currently lacks GTK4 system dev packages; GUI work
is Phase 3-gated on installing `gtk4`, `libadwaita`, `vte-gtk4`
headers. Phases 1–2 are toolkit-free by design.
