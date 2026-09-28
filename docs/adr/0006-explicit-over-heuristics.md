# ADR-0006 — Explicit signals beat heuristics; adapters own resume

- Status: accepted
- Date: 2026-09-28

Context: agent CLIs (verified Sept 2026) expose hooks carrying
`session_id` (Codex `hooks.json`/notify, Claude settings hooks,
OpenCode plugins, Cursor `hooks.json`) plus official resume flags
(`codex resume`, `claude --resume`, `opencode --session`,
`cursor-agent --resume`). Output regex is brittle and drifts.

Decision: detection priority native API → hooks → OSC 9/99/777 →
process info → terminal state → output heuristics (last resort, must
be overridable). Session ids come from hooks/env, never scraped.
Each adapter owns its official resume argv; the server persists it and
never auto-runs saved commands without explicit opt-in.

Consequences: zero-config still works (OSC/title/exit/process), but
installed hooks strictly dominate. Restore states LIVE/RESTORED/
RESUMABLE/EXITED are honest about process death.
