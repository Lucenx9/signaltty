# VTE review decisions

Independent reviews of the original implementation used `aa514e4` as their
base. Main was then refreshed to `1283514`; that update did not change
`terminal.rs`. GPT-5.6-Sol max reviewed Standards and the native boundary;
Grok 4.7 high reviewed the Spec and correctness. Both found the same
asynchronous paste defect and no other original functional defect.

## Corrections accepted

- A clipboard read could finish after exit/resume and paste into the new
  process. The native regression first observed the bytes for `STALE`, then
  passed with pending reads invalidated on exit and accepted text sent
  through VTE `paste_text`.
- Delta review caught cancellation of an earlier intentional paste when a
  second command arrived. The regression first observed one paste for two
  activations. A shared cancellable for the live period now preserves both
  requests and invalidates them together only on exit/drop.
- The parent found the menu screenshot test's undeclared ImageMagick
  dependency. Development setup, doctor and the runner fixture now declare it.
- An isolated Italian-layout probe found Ctrl+Shift+plus sending `*` to the
  child. Chords now resolve the active layout's unshifted key before shifted
  aliases; Italian Shift+0 resets rather than increasing text size. The
  regression owns a separate Xvfb server and leaves the desktop keymap alone.

The independent paste delta review by GPT-5.6-Sol max found no remaining
paste defect. It checked the shared live-period token, weak callback ownership,
RefCell lifetimes, native paste semantics and the focused regression logs.
The final keyboard delta review also found no defect in active-group key
translation, fallback ownership or the private Xvfb test's isolation.

Native regressions cover Unicode/newline and bracketed-paste bytes, no extra
Enter, repeated commands, exit/resume, dropping the pane, real Copy/Paste and
zoom chords, Menu/Shift+F10, repeated screen restores and retained history.
The full verification report and delivery proof are recorded in the PR.

## Cleanup and scope

Comment review removed four redundant descriptions and corrected the GValue
safety wording. Comments explaining external VTE/GTK behavior remain; no
constraint annotations or suppressions were added. The requested `deslop`
helper was unavailable, so cleanup used manual inspection and independent
comment review.

VTE can truncate embedded-NUL input before the callback; the adapter safely
refuses that whole message. Snapshots cannot recover a TUI's saved main grid
or a closed GUI's scrollback. Cancellation covers the new Paste commands;
existing VTE Shift+Insert and primary-selection paste retain their native
behavior. Xvfb does not certify Wayland clipboard, IME or assistive technology.
