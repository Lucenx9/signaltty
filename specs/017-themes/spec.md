# Feature Specification: GUI themes

**Feature Branch**: `017-themes`

**Created**: 2026-10-04

**Status**: Draft

**Input**: Themes for the signaltty GUI modelled on t3code appearance settings: System/Light/Dark appearance plus five named light+dark palettes chosen in a Preferences dialog and persisted as a GUI-local preference.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Choose an appearance and theme (Priority: P1)

As a user, I open Preferences from the primary (hamburger) menu, pick System,
Light or Dark, pick one of the five themes, and watch the whole window change
immediately without restarting anything.

**Why this priority**: This is the feature: choosing how the app looks and
seeing the choice apply.

**Independent Test**: Open Preferences, select each appearance and each theme
in turn, and confirm the window recolours live with no terminal restart or
scrollback loss.

**Acceptance Scenarios**:

1. **Given** the app is running, **When** I open Preferences from the primary
   menu, **Then** I see three appearance preview cards (System, Light, Dark)
   and a grid of five theme cards, each showing a light and a dark swatch,
   with the current choice marked.
2. **Given** Preferences is open, **When** I select a different theme or
   appearance, **Then** the whole window (canvas, sidebar, header bars, pane
   cards, popovers, dialogs, borders, accent) updates live with no restart.
3. **Given** System appearance is selected, **When** the desktop switches
   between light and dark, **Then** the app follows the desktop.

### User Story 2 - Keep my choice across restarts (Priority: P2)

As a user, my appearance and theme choice is still there the next time I open
the app, even if my config was missing or contained something unrecognised.

**Why this priority**: A look-and-feel choice that resets on every launch is
not a preference.

**Independent Test**: Set a non-default combination, quit, relaunch, and
confirm the same combination is active; then corrupt or delete the stored
value and confirm the app starts cleanly on System + Signal.

**Acceptance Scenarios**:

1. **Given** I selected Dark + Ocean, **When** I quit and relaunch the app,
   **Then** Dark + Ocean is active from the start.
2. **Given** the stored preference is missing, unreadable or names an unknown
   appearance or theme, **When** the app starts, **Then** it falls back to
   System + Signal (per-axis: a valid axis is kept) with no error dialog.
3. **Given** the config directory is not writable, **When** I change the
   theme, **Then** the change still applies for this session with no error
   dialog, even if it cannot be kept for the next launch.

### User Story 3 - Read statuses in any theme (Priority: P3)

As a user, I can still tell at a glance which agent needs me, in every theme
and variant, including with high contrast on and where a status colour sits
close to a theme accent.

**Why this priority**: Colour is spent on attention (docs/06); themes must not
break the attention loop that is the product (docs/14 §1).

**Independent Test**: Walk every theme in both variants, with and without
high contrast, and confirm each status meaning and the workspace marks stay
distinguishable from the theme accent.

**Acceptance Scenarios**:

1. **Given** any theme and variant, **When** panes show working, unread,
   input/warning, approval, error/failed and done states side by side,
   **Then** each meaning is visually distinguishable from the others and
   from the theme accent.
2. **Given** Ember (orange accent) is active, **When** an approval gate opens,
   **Then** the orange approval pill/ring is still distinguishable from the
   Ember accent chrome around it.
3. **Given** Grove (green accent) is active, **When** a turn completes,
   **Then** the green done marker is still distinguishable from the Grove
   accent chrome around it.
4. **Given** high contrast mode is on, **When** any theme is active,
   **Then** the existing high-contrast strengthening (full-strength text,
   stronger separators and focus markers, outlined pills) keeps working.

### Edge Cases

- Stored preference names an unknown theme id: fall back to Signal, keep the
  stored appearance if it is known, no error dialog.
- Stored preference names an unknown appearance: fall back to System, keep
  the stored theme if it is known, no error dialog.
- Config directory not writable: apply the choice for the session, skip
  persistence silently (no error dialog, no crash).
- Desktop scheme changes while System is selected: the app follows without
  user action and without touching the stored preference.
- High contrast turned on with any theme active: contrast strengthening wins
  over theme subtlety; nothing becomes unreadable.
- Status colour close to a theme accent (Ember vs orange approval, Grove vs
  green done): the status marker must remain distinguishable from the accent
  chrome by more than hue alone (e.g. shape, placement, outline or glow).
- Reduced motion on: theme switching adds no new animation; existing
  reduced-motion behaviour is unchanged.

## Requirements *(mandatory)*

**Deviation from docs/06 (Principle II justification):** docs/06 says colour
comes only from libadwaita variables so the system accent follows the
desktop. Non-Signal themes intentionally replace the desktop accent while
active. This is acceptable because Signal — the default — keeps following
the desktop exactly as today; the replacement only happens through an
explicit user choice in Preferences; and t3code, the visual bar named in
docs/14 §7, offers exactly this kind of named-theme accent. No other part
of docs/06 or docs/14 is contradicted.

### Functional Requirements

- **FR-001**: The GUI MUST offer three appearances: System, Light and Dark.
  System follows the desktop scheme; Light and Dark force the scheme.
- **FR-002**: The GUI MUST offer five named themes, each with a light and a
  dark variant: Signal (the current neutral look, the default), Grove
  (green), Ocean (blue), Ember (orange) and Iris (purple).
- **FR-003**: Selecting a theme MUST recolour the chrome: window canvas,
  sidebar, header bars, pane cards, popovers, dialogs and borders.
- **FR-004**: Selecting a theme MUST set the accent colour: Signal follows
  the desktop accent as today; Grove, Ocean, Ember and Iris replace the
  desktop accent with their own while active.
- **FR-005**: The terminal MUST take only the theme's card background, so
  the terminal grid stays visually inside its card. The 16-colour ANSI
  palette MUST be identical across all themes, so agent output such as
  red/green diffs reads the same everywhere.
- **FR-006**: A Preferences dialog opened from the primary menu MUST show
  three appearance preview cards (System, Light, Dark) and a grid of the
  five theme cards, each theme card showing a light and a dark swatch. The
  current choice MUST be marked, and selecting a card MUST apply it live to
  the whole window with no restart.
- **FR-007**: The appearance + theme choice MUST be stored in the user's
  config directory as a GUI-local preference (not server or session state)
  and restored on launch. A missing, unreadable or unknown value MUST fall
  back to System + Signal without an error dialog; each axis falls back
  independently, so a known appearance is kept when only the theme is
  unknown and vice versa.
- **FR-008**: If the choice cannot be written (e.g. config directory not
  writable), the GUI MUST still apply it for the session without an error
  dialog.
- **FR-009**: Status colours MUST keep their meaning in every theme and
  variant: amber = input/warning, orange = permission/approval, red =
  error/failed, green = done, accent = working/unread. Each MUST remain
  distinguishable from the active theme's accent and from the other
  statuses, including Ember vs orange approval and Grove vs green done.
- **FR-010**: Workspace mark tints MUST stay visually distinct from the
  active theme's accent in every theme and variant.
- **FR-011**: High contrast mode and reduced motion MUST keep working in
  every theme and variant; themes MUST NOT weaken the existing
  high-contrast strengthening nor add new animation.
- **FR-012**: Signal in Light MUST stay stock Adwaita, as today.
- **FR-013**: Switching appearance or theme MUST apply visibly within a
  fraction of a second, without restarting any terminal or losing
  scrollback.

### Key Entities

- **Appearance**: Which scheme the GUI uses: System (follow the desktop),
  Light (force light) or Dark (force dark). One value applies to the whole
  window.
- **Theme**: A named palette with a light and a dark variant that recolours
  the chrome and sets the accent: Signal (desktop accent), Grove, Ocean,
  Ember or Iris. One value applies to the whole window.
- **Preference**: The stored appearance + theme pair: GUI-local, kept in
  the user's config directory, restored on launch, silently falling back
  to System + Signal (per-axis) when missing, unreadable or unknown.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Body text meets a text/background contrast ratio of at least
  4.5:1 in every theme in both variants, light and dark.
- **SC-002**: Selecting a different appearance or theme visibly updates the
  whole window in under one second, with no terminal restart and no
  scrollback loss.
- **SC-003**: After quitting with a non-default combination selected, the
  next launch restores exactly that combination.
- **SC-004**: In every theme in both variants, each of the five status
  meanings is distinguishable from the theme accent and from the other
  statuses when shown side by side, with and without high contrast.
- **SC-005**: With high contrast on and with reduced motion on, the checks
  above still pass and no new animation is introduced.

## Assumptions

- One appearance + one theme applies to the whole window (all workspaces,
  tabs and panes); there is no per-workspace or per-pane theming.
- The desktop scheme is read through the same mechanism the app already
  uses for dark mode (mirroring the desktop style manager); no new
  desktop-integration work is implied.
- The exact storage format and file name in the config directory are left
  to the plan; only the behaviour in FR-007/FR-008 is binding.
- The exact swatch artwork in the Preferences dialog is left to the plan;
  each theme card shows a light and a dark swatch regardless of the
  currently selected appearance.
- Custom or user-defined themes, per-workspace themes, ANSI palette
  changes and font changes are out of scope.
