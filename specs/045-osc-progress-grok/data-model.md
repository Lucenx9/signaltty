# Data model

Each PTY-owned OscScanner holds zero/one bounded validated ASCII progress
payload. PtyManager reads it from the current live handle. Scanner emits a Progress event, not a notification. Missing evidence
reads as an empty string and cannot match an osc_progress region.
A spawn owns a new scanner; exit/destroy drop the handle, removing its evidence. Cold restart begins empty.
Store/Pane schemas are unchanged; lifecycle transitions still use apply_screen_state.
