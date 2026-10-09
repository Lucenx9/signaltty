# Specification: Explain a pane's screen classification

Created: 2026-10-09. Status: clarified.

Screen rules (specs 027, 028) change a pane's state from data the user cannot
see. herdr answers "why is this agent blocked?" with `agent explain`. This
adds the same read-only answer as `pane.explain`, so users and agents can
debug a rule without guessing, per docs/14 directives 3 and 4.

## Acceptance

1. `pane.explain {pane_id}` returns the pane's `kind`, `live`, `hooked`, the
   rule `source` (`user`, `bundled` or `none`), `classifies` (true when the
   screen tick would act on this pane: live, unhooked, with rules), every
   rule of that source as `{id, state, priority, region, matched}` against
   the current title and visible screen, and the winning `matched` rule
   `{id, state, priority, index}` or null (`index` is its position in
   `rules`, since rule ids may repeat across manifests).
2. It never changes pane state and emits no event. Unknown pane →
   `NO_SUCH_PANE`; bad params → `BAD_PARAMS`.
3. `signaltty pane explain ID` prints the same data (`--json` raw) with one
   line per rule, the winner marked.
4. Real-PTY integration tests cover a bundled match, a user override, a
   hooked pane (`classifies` false) and an unknown pane; docs/08 gets the row.

## Scope and clarification

Explains screen rules only; hook and process detection are reported as the
`hooked` flag and `kind`, not traced. No region text preview: `pane.read`
already returns the screen. No unresolved requirements remain.
