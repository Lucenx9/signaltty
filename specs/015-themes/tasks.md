---
description: "Task list for 015-themes implementation"
---

# Tasks: 015-themes (GUI themes)

**Input**: Design documents from `/specs/015-themes/` (`spec.md`, `plan.md`, `data-model.md`, `research.md`)

**Prerequisites**: `spec.md`, `plan.md`, `data-model.md`, `research.md`

**Organization**: Tasks are grouped by user story (US1, US2, US3) to enable independent implementation and testing of each story. CSS tasks are marked `[CSS agent]` to maintain strict isolation with the parallel CSS agent.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Core domain types module scaffolding

- [x] T001 Scaffold `crates/signaltty-core/src/theme.rs` and export `pub mod theme;` in `crates/signaltty-core/src/lib.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core domain logic and tokens that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T002 [P] Core unit tests for `theme.rs` written first (TDD) in `crates/signaltty-core/src/theme.rs`
- [x] T003 Core domain implementation: `Appearance`, `Theme`, `GuiPreference`, `pane_bg`, `parse` (per-axis fallback, corrupt JSON fallback) and `to_json` in `crates/signaltty-core/src/theme.rs`
- [ ] T004 [P] [CSS agent] Token ladder and semantic variables in `crates/signaltty-gui/data/style.css`
- [ ] T005 [P] [CSS agent] Theme selector classes (`theme-signal`, `theme-grove`, `theme-ocean`, `theme-ember`, `theme-iris`) in `crates/signaltty-gui/data/style.css`

**Checkpoint**: Core domain logic and CSS tokens ready - user story implementation can begin.

---

## Phase 3: User Story 1 - Choose an appearance and theme (Priority: P1) 🎯 MVP

**Goal**: User can open Preferences from primary menu, select System/Light/Dark and any of the 5 themes, and observe immediate live restyle across the entire window without terminal restart.

**Independent Test**: Open Preferences, switch appearance and theme, verify window classes and terminal background update live.

### Tests for User Story 1 ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T006 [P] [US1] GTK display test in `crates/signaltty-gui/src/app_tests.rs` asserting live class swapping (`theme-ocean`, `dark`, removing `theme-ocean` on switch to Signal)

### Implementation for User Story 1

- [x] T007 [US1] Update `crates/signaltty-gui/src/terminal.rs` to accept `Theme` in `apply_style` and set background from `Theme::pane_bg(is_dark)`
- [x] T008 [US1] Update `crates/signaltty-gui/src/app.rs` with `preference: RefCell<GuiPreference>`, window `theme-*` CSS class management, and `restyle_terminals()` integration
- [x] T009 [US1] Create Preferences dialog with Appearance cards/toggles and 5 Theme swatch cards in `crates/signaltty-gui/src/preferences.rs`
- [x] T010 [US1] Register `win.preferences` action (`<Control>comma`, section 3) in `crates/signaltty-gui/src/actions.rs` and wire handler in `app.rs`
- [ ] T011 [P] [US1] [CSS agent] Styling for preferences dialog, swatch cards (`theme-swatch`), and selection indicator in `crates/signaltty-gui/data/style.css`

**Checkpoint**: At this point, User Story 1 is fully functional: preferences dialog opens, choices restyle window and terminals live.

---

## Phase 4: User Story 2 - Keep my choice across restarts (Priority: P2)

**Goal**: Appearance and theme choices persist across restarts in `$XDG_CONFIG_HOME/signaltty/gui.json`, resilient to missing, corrupted, or unwritable config.

**Independent Test**: Save a custom preference, restart app, verify custom preference loaded; test corrupt file and unwritable directory fallback.

### Tests for User Story 2 ⚠️

- [x] T012 [P] [US2] Unit tests for GUI preference file loading, per-axis fallback, and write error handling in `crates/signaltty-gui/src/preferences.rs`

### Implementation for User Story 2

- [x] T013 [US2] Implement `load_preference` and `save_preference` (using `signaltty_core::paths::config_dir().join("gui.json")`) in `crates/signaltty-gui/src/preferences.rs`
- [x] T014 [US2] Integrate preference loading at startup in `App::new` in `crates/signaltty-gui/src/app.rs` before window presentation
- [x] T015 [US2] Wire `save_preference` on preference change callbacks in `crates/signaltty-gui/src/preferences.rs` / `app.rs`

**Checkpoint**: User Stories 1 AND 2 work together seamlessly; preferences persist and survive corrupted config.

---

## Phase 5: User Story 3 - Read statuses in any theme (Priority: P3)

**Goal**: Status indicators, attention markers, and high-contrast mode remain clearly distinguishable in every theme.

**Independent Test**: Walk every theme in light and dark variants, verify status pill and attention badge contrast.

### Implementation & Testing for User Story 3

- [ ] T016 [P] [US3] [CSS agent] Visual verification of agent status contrast against Ember/Grove accents and high contrast mode
- [ ] T017 [US3] [CSS agent] Adjust status pills, rings, and high-contrast CSS overrides for all 5 themes in `crates/signaltty-gui/data/style.css`

**Checkpoint**: All user stories functional and visually validated.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Workspace quality gates, formatting, and verification

- [x] T018 Code formatting with `cargo fmt --all`
- [x] T019 Workspace lints clean with `cargo clippy --workspace --all-targets`
- [x] T020 Run core unit tests and ignored GTK display test via `xvfb-run -a dbus-run-session`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: Can start immediately.
- **Foundational (Phase 2)**: Depends on Phase 1; blocks US1 and US2.
- **User Story 1 (Phase 3)**: Depends on Phase 2.
- **User Story 2 (Phase 4)**: Depends on Phase 2 & Phase 3.
- **User Story 3 (Phase 5)**: [CSS agent] Depends on Phase 2 & Phase 3 CSS tokens.
- **Polish (Phase 6)**: Depends on completion of Rust implementation phases.
