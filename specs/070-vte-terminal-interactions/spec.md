# Feature specification: reliable terminal interactions

**Feature Branch**: `t3/vte-terminal-improvements`
**Created**: 2026-10-15
**Status**: Clarified
**Input**: Implement the four terminal improvements identified in the review and open a PR.

## User scenarios & testing

### User story 1: send every key to the running program (P1)

A user can send control characters, ordinary text and pasted text to the terminal program without losing bytes available from the terminal engine.

**Independent test**: Send Ctrl+Space and ordinary Unicode input through the terminal input boundary and inspect the exact bytes delivered to the server.

**Acceptance scenarios**:
1. Ctrl+Space delivers one NUL byte instead of an empty input message.
2. A complete engine input message retains its exact bytes, including a NUL control key and non-text control bytes. If the engine has already truncated a message, the GUI refuses it instead of forwarding a partial message or reading unavailable memory.
3. Ordinary typing, control keys and multiline paste retain their existing behavior. Exited panes receive no input.

### User story 2: copy and paste through familiar controls (P1)

A user copies selected terminal text with Ctrl+Shift+C and pastes clipboard text with Ctrl+Shift+V. A terminal context menu exposes the same operations.

**Independent test**: Select terminal output, copy it to the desktop clipboard and paste known Unicode text into a live pane. Check the exact delivered content.

**Acceptance scenarios**:
1. Copy preserves the selected text. Without a selection it leaves clipboard contents unchanged and sends no input to the running program.
2. Paste respects the running program's paste mode and does not submit an extra Enter.
3. The menu names Copy and Paste, respects selection and live state, and supports keyboard invocation.
4. Plain Ctrl+C and Ctrl+V remain available to terminal programs. Other text controls retain their own clipboard behavior.
5. A clipboard read started before a pane exits cannot deliver stale text after that pane resumes.

### User story 3: read earlier output after reconnecting (P1)

A user can still scroll back through locally retained output after the GUI reconnects to the same running pane.

**Independent test**: Produce more than a screen of output, interrupt the connection, reconnect twice, then inspect history and the current screen.

**Acceptance scenarios**:
1. Already retained history survives transport reconnection while the current screen converges to the server snapshot.
2. Repeated snapshots and overlapping live output do not duplicate retained lines or live bytes.
3. Alternate-screen programs still display their current screen after reconnecting.

### User story 4: adjust terminal text size (P2)

A user can increase, decrease and reset the focused terminal's text size without changing the desktop font or maximizing the pane.

**Independent test**: Change text size in one of two panes, reset it and verify both terminal identities and the other pane's scale.

**Acceptance scenarios**:
1. Keyboard commands and the terminal context menu expose increase, decrease and reset.
2. Text size has finite bounds. Repeating a zoom command at a bound sends no input to the running program. Reset restores the desktop font's normal scale.
3. Theme changes preserve the selected scale. Split layout, search and pane zoom keep the same terminal widgets.

### Edge cases

Empty or non-text clipboard, no selection, exited panes, hidden panes, repeated reconnects, alternate screen, narrow terminals, theme changes and large text do not cause extra input or replace terminal widgets.

## Requirements

- **FR-001**: Terminal keyboard input MUST preserve complete engine messages, including the NUL control key, while retaining the existing live-pane guard. A message already truncated by the engine MUST cause no partial write or read beyond its available buffer.
- **FR-002**: Users MUST have explicit copy and paste shortcuts and an accessible terminal context menu. Paste MUST preserve terminal paste semantics.
- **FR-003**: Reconnection MUST retain previously available local scrollback and restore the canonical visible screen without duplicate output.
- **FR-004**: Users MUST have bounded text zoom and reset for the focused terminal, independent of pane zoom and desktop preferences.
- **FR-005**: These interactions MUST retain structural terminal reconciliation, existing keyboard ownership, light and dark readability, and existing server session ownership.

## Key entities

- Terminal input is a byte payload with an explicit length.
- Each pane retains its terminal widget, bounded local history and runtime text scale.
- A reconnect snapshot replaces current screen state, rather than starting another terminal session.

## Success criteria

- **SC-001**: Regression checks observe the literal expected input bytes for NUL, Unicode and multiline paste, and safely reject a message truncated by the engine.
- **SC-002**: Native clipboard and keyboard checks copy selected output and paste the exact expected text.
- **SC-003**: Two reconnects preserve an earlier history marker exactly once and display the latest screen marker exactly once.
- **SC-004**: Text zoom, reset and theme changes retain both panes and alter only the intended pane's scale.
- **SC-005**: Independent review, native light and dark inspection, full repository verification and fresh delivery proof precede PR publication.

## Assumptions

Text scale is runtime state per pane and starts at normal scale for new panes. The existing history capacity remains unchanged. This change preserves history already retained in this GUI; it does not recover missing scrollback from a closed GUI or guarantee full history generated during an outage. Hyperlink activation, history preferences and output throughput changes remain separate work.

## Clarifications

2026-10-15: The user authorized implementation and PR publication after reviewing these four priorities. Existing test seams are accepted by the repository workflow. Shortcut details and safe snapshot replacement are implementation choices to validate against GTK and real terminal behavior. No interview or issue publication is required.

2026-10-15 runtime finding: VTE 0.80.1 registers its input signal as a string plus a byte count. On a realized widget the native signal machinery can copy only the prefix before an internal NUL while retaining the original count. Both an ordinary C callback and a generic closure reproduced the loss. Recovering arbitrary bytes after that NUL would require an upstream VTE change. The original Ctrl+Space defect remains in scope; unavailable bytes must not be inferred or read. The implementation must keep this limitation explicit.
