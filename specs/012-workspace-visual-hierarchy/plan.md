# Plan: Workspace visual hierarchy

## Grounding and sketch

`App` renders cached workspace context; `Sidebar::update` reconciles stable
rows; `PaneWidget::update_meta` updates mounted cards. Keep these interfaces.
Only widget composition and scoped GTK CSS change. No new types or ownership.

Sidebar: a three-column first line (name, status, close), full-width message,
then a full-width metadata box (expanding location, trailing agent). Pane:
neutral header surface with an inset separator, then unchanged approval/output.
Workspace: branch-first subtitle with the complete path in a tooltip.

## Alternatives and synthesis

Candidate A: split the sidebar into a two-line title/activity list and a separate
workspace details toolbar. Candidate B (independent design review): retain the
three-line row but reserve close space on the first line only, and distinguish
pane headers with neutral surfaces. Choose B: more readable with smaller change,
no new toolbar or hidden metadata. Graft branch-first context from A. Reject
extra badges, glass, navigation motion and larger default sidebar width.

Tokens remain semantic: sidebar background, window canvas, view surface,
view foreground, border and accent. System sans for chrome; existing desktop
monospace for VTE. Spacing follows the existing 4/8/12 scale. Attention is the
strongest visual signal; focused headers use only a quiet accent wash.

## Sequence and verification

Capture baseline, reshape rows, refine pane styles/context, inspect native light
and dark scenes, then narrow/high-contrast/enlarged-font scenes. Run existing
allocation, focus, motion and application display tests separately (GTK thread
ownership), plus workspace build/tests/Clippy/fmt. Review diff, document evidence,
commit and push. No ADR required: existing architecture and styling policy hold.

## Sources checked 2026-09-30

- https://developer.gnome.org/hig/patterns/nav/sidebars.html — important row
  information should remain readable; list actions live above the sidebar.
- https://developer.gnome.org/hig/guidelines/ui-styling.html — native color roles,
  restrained custom styling, high-contrast testing and non-color status labels.
- https://developer.apple.com/design/human-interface-guidelines/layout — clear
  content hierarchy; translated through native Adwaita instead of glass effects.
- Local animate/apple-design/emil-design-eng/frontend-design skills: frequent
  navigation stays instant, type follows the desktop, details serve orientation.
