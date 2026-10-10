# Specification: Craft pass

**Branch**: `gui/craft-pass` | **Created**: 2026-10-10
**Status**: Implemented and verified
**Input**: "It still looks like a Rust prototype." Find what reads as unfinished in a realistic 1920×1080 scene (agents working, done, waiting; split panes with real `git log` and `ls` output) and fix it.

## Findings and fixes

### 1. Stale agent messages (P1)
A working agent's row said "session started" for its whole turn, because `SessionStart` set `last_message` and nothing cleared it.
Acceptance: A hook event that moves a pane into Working without a message of its own clears the previous message, so the row shows the run state ("Working…", "Working for 2m…") until the turn says something. A message on that same event, or later events, still wins.

### 2. Light terminals were hard to read (P1)
One GNOME palette served both schemes; its yellow (`#f5c211`, the `git log` hash colour) and bright greens nearly vanish on white.
Acceptance: Dark panes keep GNOME's palette. Light panes use a dedicated palette in which all 16 inks except the two greys read at 4.5:1 or better on white; a unit test computes WCAG contrast for each.

### 3. Cramped grid (P2)
Acceptance: Terminal rows get 1.1× leading (`set_cell_height_scale`).

### 4. Noisy pane titles (P2)
Pane headers repeated the shell's window title verbatim (`simone@luxhole: ~/.t3/…`).
Acceptance: A leading `user@host: ` prefix is dropped from the displayed title (the full working directory stays in the tooltip). Titles without that exact shape are untouched; a unit test covers the edge cases.

## Requirements
- FR-001: Server-side message clearing in the hook path only; IPC shape unchanged.
- FR-002: GUI changes confined to `terminal.rs`; no new preferences or dependencies.

## Success criteria
`hook_event_drives_codex_lifecycle` asserts the cleared message (fails without the fix). The palette contrast and title unit tests pass. Before/after 1920×1080 captures were reviewed in light and dark. `scripts/verify.sh full` passes.
