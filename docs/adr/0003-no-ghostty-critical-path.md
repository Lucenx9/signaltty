# ADR-0003 — No Ghostty dependency on the critical path

- Status: accepted
- Date: 2026-09-28

Context: only `libghostty-vt` has shipped (VT parsing/state, no
renderer); its C API is explicitly unstable with observed breaking
changes weeks apart. Full `libghostty` (rendering/config/`apprt`) is
announced, not available. Ghostty's GTK frontend is in-tree Zig with
no embedding API.

Decision: all terminal code goes through the `TerminalBackend` trait
(`HeadlessBackend` now, `VteBackend` in Phase 3). `libghostty-vt`
may be tracked behind a feature flag later, pinned to a commit, never
load-bearing. No copying of Ghostty internals.

Consequences: we ship on stable crates; Ghostty config compatibility
is desirable but subordinate to architecture correctness.
