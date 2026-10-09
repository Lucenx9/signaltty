# Plan

A pure `process_name(argv)` in `signaltty-agent` (runtime unwrap); the Store
keeps the name per process (set from spawn argv, refreshed by procscan, no
event); `Ctx::screen_rules(kind, process)` filters generic overlays and
bundled manifests by `binaries`. Bundled manifests keep their binaries.
Test-first, one slice at a time: `process_name`, bundled selection and
fixtures, Store tracking, real-PTY scoping. ADR-0028 and docs/07, docs/08
(`pane.explain.process`). Claude implements; Gemini reviews read-only.
