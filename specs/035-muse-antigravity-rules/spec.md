# Specification: Bundled screen rules for Muse and Antigravity

Created: 2026-10-10. Status: clarified.

Spec 033 scoped generic screen rules to a program and shipped five agents.
Muse and Antigravity are two more of herdr's set whose rules fit the engine.
Muse's launcher execs `muse-bin-<version>`, so its program name is never a
fixed string: `binaries` needs a prefix form.

## Acceptance

1. A `binaries` entry ending in `*` matches any program name starting with the
   text before it, both for screen-rule scoping and detection overlays. A
   `*` anywhere else, or a bare `*`, rejects the manifest.
2. signaltty bundles herdr's Muse rules (`muse`, `muse-code`, `muse-cli`,
   `muse-bin-*`) and Antigravity rules (`agy`, `antigravity`,
   `antigravity-cli`), Apache-2.0, attributed, ending in the idle fallback.
3. Fixtures, written from the screens herdr's manifests document, cover each
   rule and its near-misses; a real-PTY test classifies a `muse-bin-<version>`
   program.

## Scope and clarification

herdr requires a digit after `muse-bin-`; the prefix form also accepts
`muse-bin-` itself or `muse-bin-x`, which no real program uses. No unresolved requirements remain.
