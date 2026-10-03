# Validation guide

Build with `cargo build --workspace`. Run `cargo test -p signaltty-core diff`, `cargo test -p signaltty-server --test file_diff`, and `cargo test -p signaltty-gui` for the parser, real IPC and headless behavior.

Run `GIT_DIFF_OPTS=--unified=0 cargo test -p signaltty-server --test file_diff tracked_diff_combines_staged_and_unstaged_and_reads_deletion` to verify inherited Git display options cannot remove the promised context.

Run the ignored file-diff GTK test under an isolated display:

```sh
SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-file-diff-ui GTK_A11Y=none xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui file_diff -- --ignored --test-threads=1
```

Open Working Tree Changes, activate a file row, inspect numbered context/addition/removal lines, select/copy text and return with Back. Refresh after an edit. Binary, new, empty, unavailable and incomplete previews must show explicit states. Delay an older response while selecting/refreshing/closing and ensure it cannot replace the new reader.

Inspect the actual light/dark and 360px captures. Complete `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`. Existing ignored native-controls tests must still pass. No real provider or personal GUI session is needed.
