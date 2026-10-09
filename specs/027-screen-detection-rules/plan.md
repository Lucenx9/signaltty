# Plan

Pure rule parsing and evaluation live in `signaltty-agent` (`screen.rs`), compiled
once when a manifest loads; `regex` is the only new dependency (linear-time
matching, so user patterns cannot backtrack catastrophically). The server keeps
the gate and the transitions: `Store` records whether the current process has
seen a hook, and one `Store` transition maps a screen state to lifecycle and
attention so emits stay paired. A 500 ms server tick reads the visible screen
and title of live, unhooked panes whose kind has rules.

Write the red tests first: rule parsing/evaluation unit tests, the `Store`
transition table, then real-PTY integration tests. Record the dependency and
the gate in ADR-0024 and the manifest format in docs/07. Gemini (antigravity)
reviews the diff; Claude implements and verifies.
