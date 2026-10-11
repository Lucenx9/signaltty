# Plan

Extend feature050 geometry to application-owned workspace tools. Retain semantic palette and system fonts. Left-aligned navigation and terminal output remain the primary hierarchy. Change only CSS and explicit ownership classes. Edit existing declarations rather than stacking competing overrides; leave semantic rings and focused-pane specificity intact. Remove pane/card drop shadows but retain inset attention rings, modest inner urgency glow and separate keyboard outline. Native hit area dimensions and spacing stay unchanged.

Header controls use ghost backgrounds on native window icons and compact corners. Tabs retain native sizing, dragging and close controls with neutral selected fill and a bottom marker. Changes All/Turn remains native ToggleGroup with flat selection; menus retain native model and sizing. Search uses native SearchEntry with matching corners. Empty actions keep accent and existing native padding. No new font/color dependency.

Constitution check: spec, plan and tasks before implementation. Styling adds no behavior; use existing meaningful GTK regression tests and extend native evidence only where missing. Full gate before publication. No IPC, data model, lifecycle or ADR change.
