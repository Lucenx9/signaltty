# ADR-0025: Bundle herdr's screen rules; user rules replace them per kind

Status: proposed, 2026-10-09 (ratified on PR merge). Spec: `specs/028-bundled-screen-rules/`.

## Context

ADR-0024 made screen rules manifest data but shipped none, so a hook-less Pi
pane still showed no state out of the box. herdr maintains per-agent rules
(Apache-2.0). Its rules use nested `any`/`all`/`not` gates, `contains`, and
prompt-aware regions; signaltty's engine had only an any-of regex list over
`title` or `bottom(n)`.

## Decision

1. **Two flat gates.** A rule also takes `all` (every regex) and `not` (no
   regex). That plus splitting a herdr `any` into several rules is enough for
   Pi, OpenCode and Cursor. Nested gates and herdr's prompt-aware regions are
   not added; Claude and Codex rules, which need them, stay unported.
2. **Ported as data, attributed.** `crates/signaltty-agent/screen/*.toml` are
   embedded and parsed with `parse_manifest`, so they meet user-manifest
   validation. Each file names its herdr source commit, the Apache-2.0
   license and the translation applied (`contains` → `(?i)` escaped regex,
   `whole_recent` → `lines = 200`).
3. **Explicit idle fallback.** herdr treats a known agent that matches no
   rule as idle. Each bundled file ends with an `idle_fallback` rule at
   priority -1000 and an empty regex, so the engine itself stays "no match,
   no change".
4. **User rules replace, never merge.** If any user manifest of a kind
   declares `[[screen]]`, only user rules apply to that kind. Bundled files
   are not overlays: hook overrides keep coming from user manifests only.

## Consequences

Pi panes show working/done with no setup; OpenCode and Cursor panes get the
same until their first hook. Changing one bundled rule means copying the whole
file into the user dir. Fixtures come from the rules' text, not captured CLI
output, so drift in an agent's UI shows up as a missed state, not a test
failure. The classification tick now always runs; panes without rules are
skipped before any screen read.
