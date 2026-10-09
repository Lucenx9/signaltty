# ADR-0024: Screen-detection rules as manifest data

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/027-screen-detection-rules/`.

## Context

Hook-less agents only report title, BEL, OSC and exit, so their panes never show
`working` or `blocked`. herdr classifies them from the screen with per-agent
regex rules kept as data. ADR-0006 allows output heuristics only as the last
layer, gated behind semantic signals and overridable. ADR-0009 keeps detection
data in manifests that overlay an existing agent kind.

## Decision

1. **Rules live in manifests.** `[[screen]]` tables (`state`, `region`
   `title`/`bottom` + `lines`, `regex` list, `priority`) are compiled once when
   the manifest loads; a bad rule rejects that manifest like any other error.
   Rules for a kind come from every manifest of that kind, in file order.
2. **`regex` is the one new dependency** (in `signaltty-agent`). It matches in
   linear time, so a user pattern cannot backtrack catastrophically on screen
   text. Core stays dependency- and OS-free.
3. **Hooks dominate per process.** `Store` records when a hook reaches a
   pane's current process; from then on its screen is never classified, and
   the `input_required` a screen rule raised is withdrawn so the hook's own
   attention can apply. A new process (spawn, resume) starts unhooked.
4. **The screen never invents a state.** Only a matching rule acts: `working`
   → `working`; `blocked` → `blocked` + `input_required`; `idle` closes a
   `working` turn as `done` + `unread` and settles `blocked` as `idle`.
   Leaving `blocked` withdraws the `input_required` it raised. One
   `Store` transition applies this so lifecycle/attention emits stay paired.
5. **A server tick, not the output path.** Every 500 ms the server reads the
   headless vt100 screen and title of live, unhooked panes whose kind has rules.
   The tick starts only when some manifest declares rules.

## Consequences

Any agent can gain working/blocked/done states by dropping a TOML file, without
a release. No rules are bundled yet; porting herdr's per-agent rules (Apache-2.0)
is a follow-up with fixtures. Classification lags output by up to one tick. Rule
selection is by agent kind, so generic rules see every generic pane, shells
included; a rule that does not match changes nothing. There is no explain
method yet, so debugging a rule means reading `pane.read` against the pattern.
