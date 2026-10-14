# Verification

Run `cargo test -p signaltty-server --test file_diff selected_submodule_diff_is_not_hidden_by_git_ignore_settings -- --exact`.
The fixture uses only disposable local repositories and an isolated server.
Then stage final source and run `scripts/verify.sh full`. Check its explicit
report with `scripts/agent.sh check --phase delivery --feature 063-submodule-diff-visibility --evidence RUN`.
