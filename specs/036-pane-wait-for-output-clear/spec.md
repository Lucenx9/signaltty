# Specification: `pane.wait_for_output` and `pane.clear`

Created: 2026-10-10. Status: clarified.

herdr scripts can block until a pane prints a line and can clear a pane.
signaltty clients must poll `pane.read` for the first and cannot do the
second at all.

## Acceptance

1. `pane.wait_for_output {pane_id, match, regex?, mode?, lines?, after_seq?,
   strip_ansi?, timeout_s?}` blocks until one line of the pane's text
   contains `match` (or matches it when `regex` is true). It returns the
   `pane.read` result of the read that matched plus `matched_line`.
2. `mode` and `lines` work as in `pane.read`. In `rendered` mode only lines
   newer than `after_seq` count, and the cursor advances between polls, so a
   line already on screen never satisfies a wait started after it.
3. No match before `timeout_s` (default 3600) → `TIMEOUT`. An invalid regex →
   `BAD_PARAMS`. An unknown pane → `NO_SUCH_PANE`. A pane whose process has
   exited without a match → `PANE_EXITED`.
4. `pane.clear {pane_id}` blanks the screen and scrollback in every view:
   the server's grid and history, and attached clients, which receive the
   clear in the normal output stream with consistent offsets.
5. Both are in `docs/08`, `method::ALL`, the CLI (`pane wait-output`,
   `pane clear`) and integration tests.

## Scope and clarification

Matching is per line, like herdr. herdr re-reads its whole window each poll,
so an old matching line satisfies a new wait; `rendered` mode with
`after_seq` avoids that, and `screen`/`tail` keep herdr's behavior. A clear
that lands while the program is mid-escape-sequence can cut that sequence;
terminals recover on the next byte. No unresolved requirements remain.
