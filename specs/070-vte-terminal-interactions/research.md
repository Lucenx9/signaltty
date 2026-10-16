# Native terminal findings and synthesis

The architecture trace and two candidate designs were produced independently by Grok 4.7 high and GPT-5.6-Sol max. A fresh GPT-5.6-Sol max cross-judge scored them against exact input, history convergence, native controls, interface depth and behavioral proof. Candidate A is the base for its existing wire path and widget-local controls. Candidate B contributes the explicit exit from a stale alternate grid and the packed reconciliation probe. Broader wire-model changes, renamed entry points, global zoom and server history replay were rejected.

Both candidates' initial raw size-byte callbacks were rejected. On a realized VTE 0.80.1 widget, a native C callback and a generic closure can both receive only the C-string prefix of a payload with an internal NUL, while the declared length remains longer. Source confirms `commit` uses `G_TYPE_STRING` without static scope and emits a string pointer plus the original count. The adapter must bound the count by available C string storage, including the terminator. This fixes the original NUL control key without reading shortened allocations. Unrecoverable input is refused and documented.

Primary sources inspected at tag 0.80.1:

- [VTE signal registration](https://github.com/GNOME/vte/blob/0.80.1/src/vtegtk.cc) registers `commit` as a string and unsigned length.
- [VTE input emission and reset](https://github.com/GNOME/vte/blob/0.80.1/src/vte.cc) emits `result.c_str()` plus `result.size()` and implements reset state.
- [VTE DECSTR](https://github.com/GNOME/vte/blob/0.80.1/src/vteseq.cc) calls `reset(false, false)`, resetting modes and character substitution without clearing history.
- The locked `vte4` 0.10 generated binding converts that string to a Rust `GString` before invoking its `&str` callback.

Native repeat-snapshot experiments reject `reset(true, false)` alone because the prior current marker appears twice after two restores. Rowwise erasure, DECSTR and canonical bytes in one feed retain one history marker and one current marker. The independent probe covers both normal and initially alternate-screen widgets. The implementation must retain these assertions and cover input modes; an unrealized widget that never processes output is not acceptable evidence.

Clipboard stays with VTE's selection and paste APIs. Add only terminal-local controls and use the native context-menu model. Text scale is runtime state per pane; the desktop font and the existing 1.1 leading remain authoritative for face and spacing. Existing font-size signals can invoke the same commands without capturing additional plain Ctrl chords.

Working artifacts, prototypes, failed probes and the decision trail are under ignored `target/vte-work/`. Final regressions are committed at the existing project test seams. ADR-0040 records the decision and current snapshot limits.

## CI keyboard injection correction, 2026-10-16

The terminal chord test passed with GTK 4.18.6/VTE 0.80.1 on Debian but
failed consistently with GTK 4.22.4/VTE 0.84.0 on Ubuntu 26.04 at Shift+F10.
An isolated Ubuntu container reproduced the CI failure. Temporary event
instrumentation showed the terminal retained focus and received F10 with
Shift **and Alt** from xdotool's symbolic lookup. The test now resolves the
unmodified F10 keycode through GDK and asks xdotool to send Shift plus that
keycode. Production keyboard handling is unchanged. The existing menu-open,
Escape-dismissal and zero-child-input assertions remain the acceptance seam.
