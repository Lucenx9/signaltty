# Tasks: CLI pane identity and working directories

- [x] T001 Reproduce named attach remaining open after close and exit.
- [x] T002 Reproduce input redirection after renaming and reusing a pane name.
- [x] T003 Reproduce relative cwd launching `pwd` in the server directory.
- [ ] T004 Retain the resolved pane ID in `crates/signaltty-cli/src/attach.rs`.
- [ ] T005 Resolve explicit cwd in `crates/signaltty-cli/src/main.rs`.
- [ ] T006 Verify real CLI regressions in `crates/signaltty-cli/tests/cli.rs`.
- [ ] T007 Document cwd semantics in `docs/08-ipc.md`.
- [ ] T008 Run `scripts/verify.sh full` and review the diff before publishing.
