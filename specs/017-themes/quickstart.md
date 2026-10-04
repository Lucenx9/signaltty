# Quickstart: GUI Themes Verification Guide

**Feature**: `017-themes` | **Date**: 2026-10-04 | **Spec**: [spec.md](spec.md)

This guide describes how to run automated tests, perform manual verification, and capture the required light and dark screenshot evidence across all five themes.

---

## 1. Automated Tests

Run the test suite across core logic, headless GUI operations, and formatting/lint checks:

```sh
# Core theme models, string parsing, JSON serialization, and per-axis fallback
cargo test -p signaltty-core theme

# Terminal styling and CSS token consistency
cargo test -p signaltty-gui pane_bg_table_matches_the_stylesheet

# Complete workspace test suite
cargo test --workspace

# Linter and formatting gates
cargo clippy --workspace --all-targets
cargo fmt --check
```

### Running GTK Display Tests

To run the ignored GTK display tests verifying live preference switching on the window:

```sh
xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui theme_and_appearance_swapping -- --ignored --test-threads=1
```

---

## 2. Manual Testing Walkthrough

Start the signaltty GUI:
```sh
cargo run -p signaltty-gui
```

### Story 1: Preferences Dialog & Live Apply (FR-001, FR-002, FR-003, FR-004, FR-006, FR-013)
1. Open **Preferences**:
   - Press <kbd>Ctrl</kbd>+<kbd>,</kbd> or open the primary (hamburger) menu and click **Preferences…**.
2. **Appearance Selection**:
   - Select **Light**: Notice the entire window (canvas, sidebar, headers, cards) immediately switches to light mode without flickering or restarting terminals.
   - Select **Dark**: Notice the window immediately switches to dark mode.
   - Select **System**: Notice the window adopts your desktop system preference.
3. **Theme Selection**:
   - Click each theme card (**Signal**, **Grove**, **Ocean**, **Ember**, **Iris**).
   - Observe that the window background, sidebar, header bars, pane cards, and accent colors update instantly.
   - Observe that terminal scrollback and active shell commands continue running uninterrupted.
   - Check that the terminal grid background matches the pane card surface color exactly.

### Story 2: Persistence & Fallback (FR-007, FR-008)
1. **Normal Persistence**:
   - Set **Appearance = Dark**, **Theme = Ocean**.
   - Close signaltty and inspect `$HOME/.config/signaltty/gui.json`:
     ```json
     {"appearance":"dark","theme":"ocean"}
     ```
   - Relaunch the app. Confirm the window opens directly in **Dark + Ocean**.
2. **Independent Axis Fallback**:
   - Edit `~/.config/signaltty/gui.json` and introduce an unknown appearance:
     ```json
     {"appearance":"neon","theme":"grove"}
     ```
   - Relaunch the app. Confirm appearance falls back to **System**, while the theme remains **Grove** without any error popups.
   - Edit `~/.config/signaltty/gui.json` to introduce invalid JSON syntax:
     ```json
     {not-valid-json
     ```
   - Relaunch the app. Confirm the app launches cleanly using **System + Signal**.
3. **Unwritable Configuration Handling**:
   - Make `gui.json` read-only (`chmod 444 ~/.config/signaltty/gui.json`).
   - Open Preferences and choose **Ember**.
   - Confirm Ember applies to the active window immediately, without showing an error dialog.

### Story 3: Status Distinction & Accessibility (FR-009, FR-010, FR-011)
1. **Ember Theme Status Distinction**:
   - Activate **Ember** (burnt coral accent).
   - Verify that an approval/permission prompt (`--permission-color` / `--permission-bg-color`) displays in distinct mustard/amber, clearly distinguishable from Ember's burnt-coral accent.
   - Verify that errors remain crisp and red.
2. **Grove Theme Status Distinction**:
   - Activate **Grove** (teal accent).
   - Verify that turn completion ("Done" green dot) is distinct from Grove's teal working/accent indicators.
3. **Workspace Marks**:
   - Check workspace mark tints in Ember and Iris: ensure non-colliding replacement tints (teal and steel spare hues) are used.
4. **High Contrast Mode**:
   - Enable high contrast in desktop settings (`gsettings set org.gnome.desktop.interface high-contrast true` or via GNOME settings).
   - Confirm high contrast 1px hairlines, outlined pills, and full text opacity win over theme colors.

---

## 3. Light & Dark Screenshot Capture Matrix

Per the project constitution, all GUI changes must be verified with light and dark screenshots.

### Automated Headless Xvfb Capture Recipe

Use an isolated virtual display `:77` and `import` (from ImageMagick) to capture all 10 states (5 themes × 2 variants):

```bash
#!/usr/bin/env bash
set -euo pipefail

mkdir -p artifacts/screenshots

# Start isolated Xvfb server
Xvfb :77 -screen 0 1280x800x24 &
XVFB_PID=$!
# Scratch config dir so captures never touch the real gui.json
export XDG_CONFIG_HOME="$(mktemp -d)"
trap 'kill $XVFB_PID || true; rm -rf "$XDG_CONFIG_HOME"' EXIT

export DISPLAY=:77

THEMES=("signal" "grove" "ocean" "ember" "iris")
VARIANTS=("prefer-light" "prefer-dark")

for theme in "${THEMES[@]}"; do
  for variant in "${VARIANTS[@]}"; do
    scheme_suffix="${variant#prefer-}"
    echo "Capturing ${theme} (${scheme_suffix})..."

    # Configure target preference
    mkdir -p "$XDG_CONFIG_HOME/signaltty"
    cat <<EOF > "$XDG_CONFIG_HOME/signaltty/gui.json"
{"appearance":"${scheme_suffix}","theme":"${theme}"}
EOF

    # Launch GUI under scheme override
    ADW_DEBUG_COLOR_SCHEME="${variant}" target/debug/signaltty-gui &
    GUI_PID=$!

    # Wait for window render
    sleep 1.2

    # Capture frame
    import -window root "artifacts/screenshots/theme-${theme}-${scheme_suffix}.png"

    # Terminate GUI
    kill "${GUI_PID}"
    sleep 0.5
  done
done

echo "Screenshots successfully captured to artifacts/screenshots/"
```
