# Specification: Keep text after cancelled ANSI sequences

Created: 2026-10-29. Status: Clarified.
Input: Audit application bugs, publish separate fixes and merge after verification.

## User story

A user reading a cleaned pane title, notification or ANSI-stripped output retains the printable text after a terminal control sequence is cancelled.

## Requirements

- **FR-001**: CAN and SUB cancel an unfinished OSC or CSI during ANSI stripping. Bytes after the cancellation are ordinary text and must remain visible.
- **FR-002**: The notification sanitizer retains that text, removes cancellation/control bytes and preserves its existing length limit and trimming behavior.
- **FR-003**: Completed OSC/CSI sequences and malformed UTF-8-adjacent escape handling retain their current behavior. Unicode following cancellation remains intact.

## Acceptance and success criteria

A cancelled OSC followed by visible text must not swallow it, regardless of whether a later BEL or ST is present. A cancelled CSI must not swallow the first subsequent letter. Public strip_ansi and sanitize_notification_text regressions fail before the fix and pass afterward. Terminal tests, full verification, independent review and current GitHub checks pass before merge.

## Clarification and scope

The existing docs/10 sanitization contract, terminal behavior in docs/05 and cancellation contract of spec 074 determine the expected behavior. No new product choice, IPC, persistence, dependency, screen parser or GUI layout is introduced. Existing public sanitizer functions are the accepted test seams. No unresolved clarification remains.
