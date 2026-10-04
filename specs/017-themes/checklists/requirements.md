# Specification Quality Checklist: GUI themes

**Purpose**: Requirements-quality review of `specs/017-themes/spec.md`
**Created**: 2026-10-04
**Feature**: `specs/017-themes/spec.md`

**Note**: This checklist is a reviewer-owned requirements-quality review
artifact. `[x]` means the criterion has been reviewed and satisfied for
requirements quality; it does not mean implementation work is complete.

## Content Quality

- [x] Requirements describe behaviour without prescribing implementation (no widgets, CSS variables, file names or code structure).
- [x] The spec states the user value for each story (choosing the look, keeping it, reading statuses).
- [x] The spec is written in non-technical language a reviewer can verify without reading code.
- [x] All mandatory sections are present in template order: User Scenarios & Testing, Requirements (Functional Requirements, Key Entities), Success Criteria (Measurable Outcomes), Assumptions.

## Requirement Completeness

- [x] Zero `[NEEDS CLARIFICATION]` markers; all product-owner decisions are taken as given.
- [x] Every FR is testable (each has an observable window, dialog or launch behaviour).
- [x] Every FR is measurable or has a measurable SC backing it.
- [x] Success criteria are technology-agnostic (contrast ratios, seconds, launch behaviour, side-by-side distinguishability — no GTK/CSS terms).
- [x] Each user story has acceptance scenarios in Given/When/Then form.
- [x] Edge cases are covered: unknown theme/appearance ids, unwritable config dir, desktop scheme change under System, high contrast on, Ember-vs-approval and Grove-vs-done collisions.
- [x] Scope is bounded: custom themes, per-workspace themes, ANSI palette changes and fonts are explicitly out of scope.
- [x] Dependencies and assumptions are explicit (whole-window scope, existing scheme mechanism, storage format and swatch artwork left to the plan).
- [x] Key entities Appearance, Theme and Preference are defined with their attributes and scope.

## Feature Readiness

- [x] The Principle II deviation (replacing the desktop accent) is justified in-spec: Signal keeps following the desktop, replacement is explicit opt-in, t3code offers the same.
- [x] Invariants from the brief are all required: status meanings, mark-tint distinctness, high contrast, reduced motion, stock-Adwaita Signal light.
- [x] No contradiction with the constitution or product direction remains unjustified.
- [x] A plan can be written from this spec without further clarification.

## Notes

- Per-axis fallback (FR-007) and session-only application when unwritable (FR-008) are reviewer defaults chosen for silent-failure behaviour; flagged here for plan confirmations, not as open clarifications.
- Distinguishability "by more than hue alone" (edge cases, FR-009) is verified by side-by-side inspection per SC-004, keeping the criterion technology-agnostic.
