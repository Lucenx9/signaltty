# Plan

Move the overlay queries (`adapter_for_kind`, `screen_rules`, `detect_kind`)
onto an `Overlays` value; `Ctx` holds `RwLock<Arc<Overlays>>` and hands out
snapshots, so a reload swaps the Arc and every caller reads one consistent
set. `load_overlays` returns the overlays plus per-file failures. Add
`agents.list` / `agents.reload` handlers, proto constants, docs/08 rows and
CLI commands. Test-first through IPC. Claude implements; Gemini reviews
read-only.
