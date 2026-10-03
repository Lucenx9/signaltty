# GUI per-file diff verification

Owned files: `changes.rs`, `workspace_dialogs.rs`, `main.rs`, `actor.rs`, `app_tests.rs`, and the existing GUI stylesheet. No core/server/proto/docs/Git/task-status changes.

## Red → green

- Actor public request test initially failed waiting for a dedicated connection (ordinary control routing). After isolating `workspace.file_diff`, the same test passes: an outstanding read lasts beyond the ordinary three-second deadline while `server.status` completes, and a disconnected read reports read-specific failure without mutation “outcome unknown” wording. File-read deadline is ten seconds.
- Native GTK reader test initially failed “changed files must be keyboard activatable.” It now invokes GTK's actual `activate-cursor-row` action bound to Enter and receives exactly the selected literal path. No initial file is automatically read.
- Theme regression initially failed because addition TextTag color was identical in light/dark. Rooted CSS-variable probes and deferred weak theme updates fix it. Each theme asserts added != removed and added != normal foreground; light != dark. The fixture loads the production resource stylesheet.
- Actual closed-dialog widget destruction initially failed after pending replies were resolved, local widget references dropped (including list Refresh), clipboard providers cleared, and the native `closed` signal observed. Diagnosis showed the old dialog still retained beneath the native DialogHost. The closed handler now invalidates the controller, releases its child subtree, and disconnects global style handlers. The original reader weak reference then expires, while an independently opened replacement dialog remains correct.

## Passing focused checks

- `cargo test -p signaltty-gui`: **49 passed, 15 display tests ignored**.
- `cargo clippy -p signaltty-gui --all-targets -- -D warnings`: **passed**.
- Owned Rust files formatted with `rustfmt --edition 2021 --config skip_children=true`.
- `xvfb-run -a dbus-run-session -- env GTK_A11Y=none SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-file-diff-ui cargo test -p signaltty-gui file_diff_reader_uses_native_numbered_selectable_controls -- --ignored --test-threads=1`: **passed**.
- Independent display process: `xvfb-run -a dbus-run-session -- env GTK_A11Y=none cargo test -p signaltty-gui navigation_palette_fast_enter_and_git_dialogs_use_native_controls -- --ignored --test-threads=1`: **passed**, preserving existing Git/worktree/palette behavior.

The reader test proves literal two-hunk old/new positions and signs; non-editable native monospace selection and actual clipboard copying; preserved row instance/selection/focus/list scroll on Back and Refresh; selection/Back/Refresh/close stale-response rejection; fresh summary with invalidated patch; binary, truthful untracked addition-only truncated, empty, unchanged, unavailable and I/O-error states; 360px reachable Back/Refresh; terminal widget retention; replacement dialog isolation; and widget destruction after pending reads finish.

## Visual evidence

Directory `/tmp/signaltty-file-diff-ui` contains:

- `file-diff-light.png`
- `file-diff-dark.png`
- `file-diff-narrow.png` (360px window)
- `file-diff-incomplete.png` (untracked additions only; declared range exceeds retained lines)
- `file-diff-binary.png`

Inspected light/dark/narrow/incomplete renders. Full-width reader preserves alignment, literal `<>&` content, hunk headings, redundant signs, and reachable native controls. Semantic additions/deletions adjust to desktop theme. Root performs final integration review and workspace gates; these focused checks do not substitute for them.

Logs: `/tmp/signaltty-file-diff-headless.log`, `/tmp/signaltty-file-diff-gui-clippy.log`, `/tmp/signaltty-file-diff-gtk.log`, `/tmp/signaltty-file-diff-existing-git.log`, `/tmp/signaltty-file-diff-color-red.log`. GTK reports the existing stylesheet's unsupported `@media` rule; this feature does not alter that pre-existing rule.
