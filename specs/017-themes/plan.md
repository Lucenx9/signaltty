# Implementation Plan: GUI themes

**Branch**: `017-themes` | **Date**: 2026-10-04 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/017-themes/spec.md` and color science findings from `specs/017-themes/research.md`.

---

## Summary

Add full GUI theme and appearance support to signaltty:
1. Three appearance modes: **System** (follow desktop), **Light** (force light), and **Dark** (force dark) mapped through `adw::StyleManager`.
2. Five named themes: **Signal** (default neutral look), **Grove** (teal), **Ocean** (blue), **Ember** (coral), and **Iris** (violet) mapped via root window CSS classes (`theme-<id>`), redefining libadwaita CSS variables for chrome and accent surfaces.
3. Terminal integration: VTE background strictly follows the active theme's pane card surface (`--pane-bg-color`), while maintaining fixed 16-color ANSI palettes and text foreground.
4. Preferences dialog (`adw::PreferencesDialog`): opened via `win.preferences` (<kbd>Ctrl</kbd>+<kbd>,</kbd>) from the primary menu, offering toggle cards for Appearance and a swatch grid for Themes with live apply.
5. GUI-local persistence: stored in `$XDG_CONFIG_HOME/signaltty/gui.json` with independent per-axis fallback (System / Signal) on corrupt/missing data and silent session-only fallback on unwritable storage.

---

## Technical Context

- **Language/Version**: Rust 1.85, edition 2021.
- **Primary Dependencies**: GTK4 0.11 (GTK 4.18), libadwaita 0.9 (libadwaita 1.7.6), vte4 0.10, serde 1.0, serde_json 1.0. All existing in workspace `Cargo.toml`; zero new external dependencies.
- **Storage**: Local filesystem, `$XDG_CONFIG_HOME/signaltty/gui.json` (via `signaltty_core::paths::config_dir()`). Format is standard JSON.
- **Testing**:
  - Pure in-module unit tests in `signaltty-core` (parsing, fallback, JSON roundtripping).
  - Headless unit tests in `signaltty-gui` (action registry, preference file I/O).
  - CSS consistency tests validating Rust token tables against `data/style.css`.
  - Ignored GTK display tests in `signaltty-gui/src/app_tests.rs` run under `xvfb-run`.
- **Target Platform**: Linux native desktop (Wayland-first, X11 fallback).
- **Project Type**: Desktop GUI client in a multi-crate workspace.
- **Performance Goals**:
  - Live appearance or theme switch visible in < 100ms; full window repaint in < 1s (SC-002).
  - Zero terminal restarts, zero dropped VT connections, zero scrollback loss during theme/appearance change (FR-013, SC-002).
- **Constraints**:
  - Pure domain logic isolated in `signaltty-core` with no GTK/async/OS dependencies (Constitution Principle V).
  - ANSI 16-color palette in `terminal.rs` remains completely unchanged (FR-005).
  - Signal light mode remains stock Adwaita (FR-012).
  - Missing, invalid, or unwritable config handled silently without error dialogs (FR-007, FR-008).
  - Contrast ratios for body text and status indicators meet WCAG \ge 4.5:1 in all variants (SC-001, SC-004).

---

## Constitution Check

*GATE: All principles from `.specify/memory/constitution.md` evaluated.*

| Principle / Gate | Status | Plan Compliance |
|---|---|---|
| **I. Spec-driven, always** | **PASS** | Specification `specs/017-themes/spec.md` is complete; requirements FR-001..FR-013 and success criteria SC-001..SC-005 drive all planned deliverables. |
| **II. Product bar (docs/14 §7)** | **PASS** | Linear-grade tokens via libadwaita CSS variable redefinition; 5 themes modelled on t3code palettes; status vocabulary and attention loop preserved. |
| **III. Skills are load-bearing** | **PASS** | Architectural and codebase design practices applied; phased plan with strict TDD order. |
| **IV. Test-first (NON-NEGOTIABLE)** | **PASS** | Red-green-refactor cycle: core types and fallback logic tested first, then CSS validation test, then GUI integration and GTK display tests. |
| **V. Deep modules, typed boundaries** | **PASS** | `Appearance`, `Theme`, `GuiPreference`, and `ThemeTokens` live in `signaltty-core::theme` with pure unit tests. `signaltty-gui` interacts via typed enums. |
| **VI. Decisions on record** | **PASS** | Deliverable includes ADR `docs/adr/0019-gui-themes.md` documenting libadwaita variable overrides, accent replacement, and per-axis fallback. |
| **VII. Simplicity over scaffolding** | **PASS** | Reuses `serde_json` (no `toml` crate addition); static card hex lookup verified by test (no fragile GTK4 CSS runtime querying); smallest change that satisfies spec. |
| **Quality Gates** | **PASS** | `cargo fmt --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace` all green; light + dark screenshots verified via Xvfb. |

---

## Project Structure

### Documentation (this feature)

```text
specs/017-themes/
├── plan.md              # This implementation plan
├── research.md          # Color science, contrast ratios, and token values
├── data-model.md        # Appearance, Theme, and GuiPreference schemas & fallback
└── quickstart.md        # Manual verification and screenshot capture guide
```

### Source Code Touched / Added

```text
crates/signaltty-core/
├── src/
│   ├── theme.rs         # NEW: Appearance, Theme, GuiPreference, ThemeTokens, fallback logic & tests
│   └── lib.rs           # Export pub mod theme

crates/signaltty-gui/
├── data/
│   └── style.css        # Theme classes (.theme-*), status variables, high-contrast cascade
├── src/
│   ├── actions.rs       # Add win.preferences action (Ctrl+,) to action registry
│   ├── app.rs           # Appearance & theme state, CSS class toggling, StyleManager sync
│   ├── app_tests.rs     # GTK display test verifying live theme & appearance apply
│   ├── preferences.rs   # NEW: adw::PreferencesDialog UI, swatch rendering, file load/save
│   └── terminal.rs      # Pane card background lookup from ThemeTokens in apply_style

docs/adr/
└── 0019-gui-themes.md   # NEW ADR: Themes via libadwaita CSS variable redefinition & accent override
```

---

## Architectural Decisions & Grounding

### 1. Persistence Format & Storage

- **Path**: `$XDG_CONFIG_HOME/signaltty/gui.json` (or `~/.config/signaltty/gui.json` via `signaltty_core::paths::config_dir()`).
- **Format**: JSON (`serde_json`), which is already a workspace dependency. Zero new crates.
- **Independent Per-Axis Fallback**:
  Deserialization uses custom logic or helper methods:
  - If `"appearance"` is missing or invalid: `Appearance::System`.
  - If `"theme"` is missing or invalid: `Theme::Signal`.
  - Valid axes are preserved independently (e.g. `{"appearance":"dark","theme":"unknown"}` -> `Dark` + `Signal`).
  - Corrupt file or empty content -> `(System, Signal)`.
- **Silent Unwritable Fallback**:
  If file writing fails (read-only filesystem, permissions), the error is logged at debug level and silently swallowed. The user selection remains active in memory for the current session without popping error dialogs.

### 2. Terminal Background Mechanism

- **Chosen Approach**: A small static lookup table in `signaltty_core::theme::ThemeTokens` returning the card background hex per `(Theme, is_dark)`, consumed directly by `terminal.rs::PaneWidget::apply_style`.
- **Justification over Dynamic CSS Querying**:
  In GTK4, querying computed CSS node styles from a live widget is deprecated and unreliable (`gtk_style_context_get_color` only returns foreground; background queries require inspecting internal CSS render nodes). Dynamic querying introduces timing hazards and layout race conditions.
- **Safety Mechanism**:
  A dedicated unit test in `signaltty-gui` reads `data/style.css` and asserts that every `--pane-bg-color` declared in the CSS matches the static Rust table byte-for-byte. This guarantees zero drift at compile/test time with zero runtime overhead.

| Theme | Light Pane Card Hex | Dark Pane Card Hex | VTE Text Foreground |
|-------|--------------------|-------------------|---------------------|
| Signal | `#ffffff` | `#111114` | Light: `#241f31`, Dark: `#deddda` |
| Grove  | `#effdfd` | `#071414` | Light: `#241f31`, Dark: `#deddda` |
| Ocean  | `#f3fbff` | `#091318` | Light: `#241f31`, Dark: `#deddda` |
| Ember  | `#fff8f5` | `#180f0b` | Light: `#241f31`, Dark: `#deddda` |
| Iris   | `#fbf8ff` | `#131018` | Light: `#241f31`, Dark: `#deddda` |

### 3. CSS Cascade & Selector Specificity

The application window has the following root CSS classes:
- Active theme: `theme-signal`, `theme-grove`, `theme-ocean`, `theme-ember`, or `theme-iris`.
- Active scheme: `.dark` (added by `sync_desktop_preferences` when `StyleManager::is_dark()` is true).
- Accessibility: `.high-contrast` and `.reduced-motion`.

#### Selector Rules & Specificity Ladder
1. **Light Theming**:
   `.theme-grove`, `.theme-ocean`, `.theme-ember`, `.theme-iris` (specificity `(0, 1, 0)`) define light CSS variables (`--window-bg-color`, `--sidebar-bg-color`, `--pane-bg-color`, `--accent-bg-color`, `--accent-color`, etc.).
   `.theme-signal` does not set light variables, keeping stock Adwaita defaults (FR-012).
2. **Dark Theming**:
   `.dark` (existing rule) sets dark variables for Signal.
   `.theme-grove.dark`, `.theme-ocean.dark`, `.theme-ember.dark`, `.theme-iris.dark` (compound class, specificity `(0, 2, 0)`) override variables for coloured dark variants.
3. **High Contrast Guarantee (SC-005)**:
   In `data/style.css`, the high-contrast block is placed **strictly after** all theme blocks in source order:
   ```css
   .high-contrast {
     --border-color: alpha(currentColor, 0.55);
     --border-opacity: 50%;
     --sidebar-border-color: alpha(currentColor, 0.55);
     --headerbar-shade-color: alpha(currentColor, 0.55);
   }
   .dark.high-contrast {
     --border-color: alpha(currentColor, 0.55);
     --sidebar-border-color: alpha(currentColor, 0.55);
     --headerbar-shade-color: alpha(currentColor, 0.55);
   }
   ```
   Because source order governs equal specificity (`(0, 2, 0)`), high-contrast borders and dividers always win over theme variables.
4. **Status Collisions & Ember Overrides (FR-009)**:
   Ember's burnt coral accent (hue 46) would collide with orange approval and amber warning. Base status rules in `style.css` are updated to use abstract tokens:
   - `--permission-color`: text color (default light `#c64600`, dark `#ffa348`; Ember light `#875905`, dark `#d19845`).
   - `--permission-bg-color`: ring/wash color (default `#ff7800`; Ember `#9b670b`).
   Ember overrides `--warning-color`, `--warning-bg-color`, `--error-color`, `--error-bg-color`, and permission tokens. This ensures approval rings, warning pills, and error badges remain unmistakably distinct without duplicating widget rules.
5. **Workspace Mark Tints (FR-010)**:
   - Dark mark glyphs use lighter values (`alpha(fill, 0.45)`) across all dark themes to achieve \ge 8.3:1 contrast against dark sidebars.
   - For Ember: tint-1 is replaced with teal spare (`#005355` / `#4cdce0`), tint-2 with steel spare (`#004f8b` / `#96c9fe`).
   - For Iris: tint-0 is replaced with teal spare, tint-2 with steel spare.

### 4. Preferences Dialog UI Design

- **Component**: `adw::PreferencesDialog` presented over the main window.
- **Action**: `win.preferences` registered in `crates/signaltty-gui/src/actions.rs` with accelerator `<Control>comma`, placed in section 3 of the primary menu.
- **Group 1: Appearance**:
  - Three selectable preview cards: "System", "Light", "Dark".
  - Shows an icon/glyph representing the mode, label, and an active check/selection state.
  - Activating triggers `adw::StyleManager::default().set_color_scheme()`.
- **Group 2: Theme**:
  - Grid/FlowBox containing 5 theme cards: Signal, Grove, Ocean, Ember, Iris.
  - Each card contains:
    - Theme title.
    - Dual swatch: a preview container displaying the Light variant swatch on the left and Dark variant swatch on the right (showing the theme's background, card surface, and accent dot).
    - Active selection indicator.
  - Activating triggers live update of the root window's `theme-<id>` class and re-styling of all terminals.

---

## Phased Implementation Plan & TDD Order

### Phase 1: Core Domain Types & Fallback (signaltty-core)
- **Test First**: Write `crates/signaltty-core/src/theme.rs` unit tests:
  - `Appearance`: string roundtrips, unrecognized string fallback to `System`.
  - `Theme`: string roundtrips, CSS class name generation, fallback to `Signal`.
  - `GuiPreference`: serialization to JSON, deserialization from valid JSON, independent per-axis fallback for partial or invalid JSON inputs, corrupt JSON handling.
  - `ThemeTokens`: table lookup tests for card background hexes across all 5 themes.
- **Implementation**: Implement `Appearance`, `Theme`, `GuiPreference`, `ThemeTokens` in `crates/signaltty-core/src/theme.rs` and export from `crates/signaltty-core/src/lib.rs`.

### Phase 2: Stylesheet Expansion & CSS Verification (signaltty-gui)
- **Test First**: Write a unit test asserting that `data/style.css` contains valid theme definitions for all 5 themes and that `--pane-bg-color` matches `ThemeTokens` values.
- **Implementation**:
  - Update `crates/signaltty-gui/data/style.css`:
    - Refactor base status definitions to consume `--permission-color` and `--permission-bg-color`.
    - Add `.theme-grove`, `.theme-ocean`, `.theme-ember`, `.theme-iris` and their `.dark` counterparts using tokens from `research.md`.
    - Add Ember status overrides and workspace mark tint overrides.
    - Ensure `.high-contrast` and `.dark.high-contrast` cascade rules are positioned at the bottom.

### Phase 3: Terminal Pane Styling (signaltty-gui)
- **Test First**: Unit test verifying `Scheme::for_theme(theme, is_dark)` produces the exact background from `ThemeTokens`.
- **Implementation**:
  - Update `crates/signaltty-gui/src/terminal.rs`:
    - Extend `apply_style(&self, theme: Theme)` to query `ThemeTokens::pane_bg_hex(theme, is_dark)`.
    - Apply foreground (`#241f31` light, `#deddda` dark) and theme card background into `term.set_colors`.

### Phase 4: App Shell Wiring & Action Registry (signaltty-gui)
- **Test First**: Unit test in `actions.rs` verifying `win.preferences` action registration, shortcut `<Control>comma`, and primary menu entry.
- **Implementation**:
  - In `crates/signaltty-gui/src/actions.rs`:
    - Add `HandlerKind::Preferences`.
    - Add `ActionDef` for `preferences` with accel `<Control>comma` in section 3.
  - In `crates/signaltty-gui/src/app.rs`:
    - Add current `Theme` and `Appearance` to `App` model.
    - Load initial preference from disk on startup; apply scheme to `adw::StyleManager` and CSS class to window.
    - Implement `set_theme` and `set_appearance` methods on `App`:
      - Update window CSS classes (`remove_css_class(old)`, `add_css_class(new)`).
      - Trigger `restyle_terminals()`.
      - Persist preference to disk (with silent error handling).

### Phase 5: Preferences Dialog UI (signaltty-gui)
- **Implementation**:
  - Create `crates/signaltty-gui/src/preferences.rs`:
    - Build `adw::PreferencesDialog` with Appearance and Theme groups.
    - Render dual-swatch cards for each theme.
    - Connect card activation to `app.set_theme` and `app.set_appearance`.
  - Wire dialog launch to `win.preferences` action in `app.rs`.

### Phase 6: GTK Display Test, ADR & Evidence
- **Display Test**: Add GTK display test in `crates/signaltty-gui/src/app_tests.rs` verifying that switching appearance and theme live updates CSS classes and triggers terminal restyling without rebuilding widgets.
- **ADR**: Write `docs/adr/0019-gui-themes.md` summarizing the load-bearing design decisions.
- **Evidence**: Run the automated Xvfb script from `quickstart.md` to capture light and dark screenshots for all 5 themes.

---

## Test Strategy

1. **Unit Tests (Core)**:
   - `test_appearance_from_str_and_fallback`: checks valid strings, case handling, and unknown value fallback to `System`.
   - `test_theme_from_str_and_fallback`: checks 5 themes and fallback to `Signal`.
   - `test_preference_serde_and_independent_fallback`: validates independent axis recovery (unknown appearance with valid theme, valid appearance with unknown theme, corrupt JSON).
   - `test_theme_tokens_match_palette`: validates card hexes.
2. **Headless Tests (GUI)**:
   - `test_style_css_matches_theme_tokens`: asserts `style.css` contains exact `--pane-bg-color` matching `ThemeTokens`.
   - `test_action_registry_includes_preferences`: checks `ACTIONS` table integrity.
   - `test_preference_persistence_read_write`: validates atomic save and corrupted file recovery using temporary directories.
3. **Display Tests (GTK Headless via Xvfb)**:
   - `preference_appearance_and_theme_applies_live`: launches headless `App`, invokes `set_appearance` and `set_theme`, asserts CSS classes on `app.window` and verified terminal style update without rebuilding panes.

---

## Verification & Screenshots Recipe

To satisfy Constitution Quality Gate 3 (light + dark screenshots):

```sh
# Ensure debug binary is built
cargo build -p signaltty-gui

# Launch Xvfb server on display 77
Xvfb :77 -screen 0 1280x800x24 &
XVFB_PID=$!

# For each theme and appearance:
DISPLAY=:77 ADW_DEBUG_COLOR_SCHEME=prefer-light target/debug/signaltty-gui &
GUI_PID=$!
sleep 1
import -window root artifacts/screenshots/theme-ocean-light.png
kill $GUI_PID

DISPLAY=:77 ADW_DEBUG_COLOR_SCHEME=prefer-dark target/debug/signaltty-gui &
GUI_PID=$!
sleep 1
import -window root artifacts/screenshots/theme-ocean-dark.png
kill $GUI_PID

kill $XVFB_PID
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| **High contrast overridden by theme CSS** | High contrast users lose visibility of separators and outlines. | Keep high contrast block at the end of `style.css` and use specific selectors (`.dark.high-contrast`, `.high-contrast`) so it outranks theme blocks. |
| **Terminal background mismatch** | VTE terminal appears as an unintegrated block inside the pane card. | Static `ThemeTokens` table used by `terminal.rs` and validated against `style.css` in an automated unit test. |
| **Unwritable config crashing the GUI** | User on read-only mount or strict permission sandbox cannot run the app. | Persistence helper discards write errors silently and keeps preference active in-memory for the session (FR-008). |
| **Status confusion in Ember/Grove** | Attention loops degraded if approval looks like accent or done looks like accent. | Grove accent is hue 198 (teal) vs green 158 done; Ember explicitly retints approval to hue 74 mustard and error to hue 16 red, clearing \ge 14 \Delta E. |
