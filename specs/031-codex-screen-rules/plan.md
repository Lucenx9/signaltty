# Plan

Extend the region slice with the three prompt-marker bases and a `top`
direction (first instead of last `lines`); prompt detection follows herdr's
`codex_prompt_line` and `codex_block_marker_line`. Port `codex.toml`, splitting
herdr's nested any/all gates into rules or alternations as in specs 028/030.

Test-first: region unit tests, then Codex fixtures. Record the regions in
ADR-0027 and docs/07. A Claude review agent reviews the diff; no Gemini.
