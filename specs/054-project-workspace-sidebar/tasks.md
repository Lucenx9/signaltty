# Tasks: Selectable project sidebar

## Grounding and contracts

- [x] T001 Inspect docs/constitution, T3Code sidebar selectors, skills and source trace; specify spec054.
- [x] T002 Compare server-metadata/client-IPC sketches and independent Gemini judgment; write plan, model, contracts and ADR0033.
- [x] T003 Capture native parent baseline /tmp/project-sidebar-baseline and verify doctor.

## US2 Canonical membership

- [x] T004 Add failing realGit main/subdirectory/linked-worktree assertion in crates/signaltty-server/tests/worktrees.rs and pure catalog/scope tests in core/project.rs.
- [x] T005 Add defaulted GitInfo.primary_root and pure project identities/catalog/scope in crates/signaltty-core/src/model.rs, project.rs, lib.rs.
- [x] T006 Resolve primaryroot from NUL worktree records in server/git.rs; pair legacybackfill through Store transition and finite cancellable post-listen queue in server/store.rs, server.rs.
- [x] T007 Verify legacy snapshot compatibility, backfill stale-cwd/event behavior and restart through server tests.

## US1 Native selection

- [x] T008 Add failing GTK selector/search/filter/identity regression in gui/app_tests.rs before UI code.
- [x] T009 Build native project_scope selector and isolated client persistence in gui/project_scope.rs; register in main.rs.
- [x] T010 Integrate full-row scope projection and combined task collapse in gui/sidebar.rs; pin selector/authority catalog in app.rs and refresh.rs.
- [x] T011 Verify no-match/All/Escape/keyboard, row and VTE identity, valid/disconnected scope, duplicate/long names, orphan task workers.

## US3 Creation and global navigation

- [x] T012 Test selected-project NewWorkspace default, retained draft/pending safety and hidden global attention/palette navigation in app_tests.rs.
- [x] T013 Integrate selected-folder default and explicit target reveal in app.rs while refresh remains scope-neutral.

## Delivery

- [x] T014 Update docs02/06/08/09 and verification feature map; retain handoff and reviewed native before/after QA evidence.
- [ ] T015 Run clean scripts/verify.sh full and native high contrast; inspect light/dark/360px/Sans18/long/duplicate/RTL scenes.
- [ ] T016 Final Gemini3.8FlashHigh Spec/UX and independent Standards review in full-access; fix findings and record verification.md.
- [ ] T017 Publish/link PR against main, ensure clean checkout and close delivery handoff.

## Dependencies and ownership

T001-T003 before tests/source. Red metadata tests before model/resolver; red GTK before widgets. Canonical membership precedes selector integration. One repository writer; independent design/review outputs isolated in/tmp. No parallel implementation sharing files. Stories converge on app/sidebar and must preserve fullcache ownership. No extension hooks configured. Final review uses user's Gemini provider/mode preference.
