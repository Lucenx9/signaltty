# ADR-0009: Manifest Detection Overlays + Live /proc Refresh

**Status**: Accepted (2026-09-29) · **Spec**: `specs/005-agent-manifests-proc/`

## Context

Directive 3 (docs/14 §3) wants detection-as-data: herdr's
`src/detect/manifests/*.toml`, community-extensible. All signaltty
adapters were hardcoded Rust, and spawn argv was the only process
signal — docs/02 and docs/07 promised cwd/kind tracking that did not
exist.

## Decision

1. **Overlays, not a plugin taxonomy.** A manifest extends exactly one
   existing `AgentKind` (extra binaries, per-hook lifecycle/attention/
   message overrides, session payload key, resume argv template,
   display-name override). Everything unmapped falls through to the
   builtin field-by-field. New agent families still need a Rust adapter:
   the taxonomy stays closed and typed.
2. **No globals, no OS in the agent crate.** `parse_manifest(&str)` and
   the concrete `OverlayAdapter: AgentAdapter` are pure; the server reads
   the dir (per-file failure isolation, logged skips) and routes through
   one `Ctx::adapter()` helper. Spawn detection uses
   `detect_kind_with_overlays`.
3. **Promote-only live refresh.** A 10s tick over live panes reads the
   deepest shell-transparent descendant and `/proc/<pid>/cwd`:
   `Generic/None → specific` promotion, cwd follow, one `pane.updated`
   per changed pane (existing event, no new types), persist only on
   change. Never demote a hooked/known agent; shell-skipping is one
   documented level, not a process-tree oracle.

## Consequences

- CLI churn (renamed binaries, new hook names) is a TOML drop, not a
  release. `integration status` shows manifests alongside shims.
- Shell-hosted agents (`sh -c '…codex…'`) classify correctly within ~10s
  with zero hooks.
- Malformed manifests cannot prevent startup (tested).
