# Data Model: GUI Themes & Preferences

**Feature**: `015-themes` | **Date**: 2026-10-04 | **Spec**: [spec.md](spec.md)

This document specifies the domain types, persistence schema, validation rules, and per-axis fallback semantics for GUI appearance and themes. Pure logic lives in `signaltty-core::theme` without async, OS, or GUI toolkit dependencies.

---

## 1. Domain Entities

### `Appearance`

Controls the color scheme preference of the application window.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
```

- **Variants**:
  - `System` (default): Follows the operating system desktop color scheme via `adw::StyleManager::color_scheme = ColorScheme::Default`.
  - `Light`: Forces light mode via `adw::StyleManager::color_scheme = ColorScheme::ForceLight`.
  - `Dark`: Forces dark mode via `adw::StyleManager::color_scheme = ColorScheme::ForceDark`.
- **String Identifiers**: `"system"`, `"light"`, `"dark"`.
- **Conversion & Fallback**:
  - `Appearance::from_str_or_fallback(s: &str) -> Appearance`: returns `Appearance::System` on unrecognised inputs.

---

### `Theme`

Controls the named palette that recolours the chrome (canvas, sidebar, header bars, pane cards, popovers, dialogs, borders) and sets the accent color.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Signal,
    Grove,
    Ocean,
    Ember,
    Iris,
}
```

- **Variants & CSS Class Identifiers**:
  - `Signal` (default): `"signal"` -> CSS class `.theme-signal`. Neutral palette; follows desktop system accent; light variant uses stock Adwaita.
  - `Grove`: `"grove"` -> CSS class `.theme-grove`. Emerald-teal palette (hue 198); custom teal accent.
  - `Ocean`: `"ocean"` -> CSS class `.theme-ocean`. Cerulean palette (hue 228); custom blue accent.
  - `Ember`: `"ember"` -> CSS class `.theme-ember`. Burnt coral palette (hue 46); custom orange-sienna accent; retinted warning (110), permission (74), and error (16) statuses.
  - `Iris`: `"iris"` -> CSS class `.theme-iris`. Violet palette (hue 302); custom purple accent; retinted violet workspace mark.
- **Conversion & Fallback**:
  - `Theme::from_str_or_fallback(s: &str) -> Theme`: returns `Theme::Signal` on unrecognised inputs.
- **Display Properties**:
  - `theme.label() -> &'static str`: `"Signal"`, `"Grove"`, `"Ocean"`, `"Ember"`, `"Iris"`.
  - `theme.css_class() -> &'static str`: `"theme-signal"`, `"theme-grove"`, `"theme-ocean"`, `"theme-ember"`, `"theme-iris"`.

---

### `ThemeTokens` & Swatch Metadata

Static palette definition for terminal backgrounds and UI preview cards (mirrored from `research.md`).

```rust
pub struct ThemeTokens {
    pub name: &'static str,
    pub light_card_bg: &'static str,
    pub dark_card_bg: &'static str,
    pub light_accent_bg: &'static str,
    pub dark_accent_bg: &'static str,
    pub light_preview_bg: &'static str,
    pub dark_preview_bg: &'static str,
}
```

| Theme | Light Card Hex (`--pane-bg-color`) | Dark Card Hex (`--pane-bg-color`) | Accent Bg Hex (`--accent-bg-color`) |
|-------|------------------------------------|-----------------------------------|--------------------------------------|
| **Signal** | `#ffffff` | `#111114` | *Desktop default* (`#3584e4`) |
| **Grove**  | `#effdfd` | `#071414` | `#097e82` |
| **Ocean**  | `#f3fbff` | `#091318` | `#097a9e` |
| **Ember**  | `#fff8f5` | `#180f0b` | `#bc5107` |
| **Iris**   | `#fbf8ff` | `#131018` | `#895ac3` |

Terminal foreground remains `#241f31` in light mode and `#deddda` in dark mode across all themes. The 16 ANSI colors remain fixed and identical.

---

### `GuiPreference`

The persisted user configuration.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GuiPreference {
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub theme: Theme,
}
```

---

## 2. Persistence Specification

- **File Path**: `$XDG_CONFIG_HOME/signaltty/gui.json` (or `~/.config/signaltty/gui.json` via `signaltty_core::paths::config_dir()`).
- **Format**: UTF-8 JSON.
- **Schema Example**:
  ```json
  {
    "appearance": "dark",
    "theme": "ocean"
  }
  ```

### Deserialization & Per-Axis Fallback Rules

Per FR-007, each axis falls back independently:
1. **Missing File**: Returns `GuiPreference::default()` (`System`, `Signal`). No error dialog.
2. **Corrupted / Invalid JSON**: Returns `GuiPreference::default()`. No error dialog.
3. **Invalid Appearance Value**:
   - Input: `{"appearance": "neon", "theme": "grove"}`
   - Result: `appearance` falls back to `Appearance::System`; `theme` retains `Theme::Grove`.
4. **Invalid Theme Value**:
   - Input: `{"appearance": "light", "theme": "solarized"}`
   - Result: `appearance` retains `Appearance::Light`; `theme` falls back to `Theme::Signal`.
5. **Partial / Missing Keys**:
   - Input: `{"theme": "ember"}` -> `appearance: System`, `theme: Ember`.
   - Input: `{}` -> `appearance: System`, `theme: Signal`.
6. **Unknown Keys**: Unrecognized JSON keys are ignored during deserialization (forward compatibility).

### Serialization & Write Semantics

- **Trigger**: Saved synchronously/locally whenever the user selects a new appearance or theme in the Preferences dialog.
- **Directory Creation**: Ensures parent directory exists (`std::fs::create_dir_all(config_dir)`).
- **Atomic / Safe Write**: Writes formatted JSON to `gui.json`.
- **Unwritable Handling (FR-008)**: If writing fails (e.g. read-only filesystem, permission denied), the error is logged at debug/warn level, but silently ignored. No error dialog is displayed to the user; the choice remains active for the current session.
