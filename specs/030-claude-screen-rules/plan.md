# Plan

Replace `Region::Bottom(n)` with a base region plus an optional line limit;
the base slices raw screen lines (prompt box, above box, after last rule),
then the existing normalization (trim, drop empty lines, keep the last n)
applies. Add `ScreenState::Hold`; `Store::apply_screen_state` treats it as a
no-op. Port `claude.toml` into `crates/signaltty-agent/screen/`, flattening
herdr's nested gates with alternations and order-insensitive `(?s)` pairs.

Test-first: region and hold unit tests, Store hold test, Claude fixtures.
Record the regions and `hold` in ADR-0026 and docs/07. A Claude review agent
reviews the diff; no Gemini.
