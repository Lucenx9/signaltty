# 06 — GUI Toolkit Decision: GTK4/libadwaita (over Qt 6/QML)

Evaluated against: Wayland-first, KDE Plasma + GNOME, GPU rendering,
keyboard/IME/clipboard, splits, custom drawing (attention rings),
accessibility, notifications, and Rust binding maturity.

## GTK4 + libadwaita (gtk4-rs + relm4) — RECOMMENDED

- Mature Rust bindings (`gtk4` 0.10/0.11, `libadwaita` 0.8/0.9,
  `vte4` 0.9/0.10), GNOME 50 runtime, GTK 4.24 Wayland improvements
  (fractional scaling, text-input v3.2, ext-background-effect).
- **VTE terminal widget exists** — no other toolkit gives us a native
  terminal renderer with a11y/IME/selection for free.
- Wayland is GTK's primary backend; X11 fallback works; runs fine on
  Plasma (GJ issues are theming-level, not functional).
- Custom drawing for attention rings: `GtkDrawingArea`/CSS borders —
  trivial.
- Desktop notifications: `notify-rust`/`ashpd` integrate regardless of
  toolkit; `.desktop` + StartupWMClass story is standard.
- Relm4 (Elm-style) keeps Rust core independent: core crates stay
  toolkit-free, GUI crate depends on core types only.

## Qt 6 / QML (cxx-qt) — REJECTED for now

- `cxx-qt` is KDAB-maintained but self-described early-stage; supports
  QtCore/QtGui/QML only (no Widgets/KDE Frameworks bindings).
- Real-world fragility confirmed in 2026 reports: link-flag
  incompatibilities against newer Qt (6.11), per-module bridge layout
  constraints (QTBUG-93443 workarounds), C++ toolchain in build.
- **No terminal widget**: we'd hand-roll QML terminal rendering
  (fonts/shaping/IME/a11y) — the single biggest GUI risk, with zero
  reuse.
- KDE integration would be nicer (Kirigami, layer-shell), but not
  worth owning a terminal renderer.

## Decision

Build the Phase 3 GUI on **GTK4 + libadwaita + VTE via gtk4-rs/relm4**,
with all logic in toolkit-independent core crates. Keep the
`TerminalBackend` trait (see [05](05-terminal-backend.md)) so a Qt or
custom-GPU frontend stays possible without touching the server.

Note: this machine currently lacks GTK4 system dev packages; GUI work
is Phase 3-gated on installing `gtk4`, `libadwaita`, `vte-gtk4`
headers. Phases 1–2 are toolkit-free by design.

## Interface design

Layout follows libadwaita idiom so the app behaves like its neighbours
on GNOME and Plasma: `AdwOverlaySplitView` (sidebar collapses to an
overlay below 760sp), `AdwTabView`/`AdwTabBar` (autohides with one
tab), one primary menu, every command a `win.*` action with an
accelerator. Colour comes only from libadwaita variables, so light,
dark, the system accent and high contrast follow the desktop;
terminals take the desktop monospace font and a matching palette.

One status vocabulary, shared by every surface
(`crates/signaltty-gui/src/status.rs`, colours in `data/style.css`):

| Axis | Sidebar status slot | Pane header | Tab | Pane card |
|---|---|---|---|---|
| lifecycle `working` | spinner + "Working" (accent) | spinner | tab spinner | — |
| lifecycle `blocked`/`failed` | "Waiting" / "Failed" (amber / red) | amber / red dot | — | — |
| lifecycle `done` | "Done" (green) while unread, else time | green dot | — | — |
| attention `unread` | accent dot + time | accent dot | indicator icon | hairline accent ring |
| attention `input`/`warning` | "Input"/"Warning" (amber) | pill | icon + bar glow | amber ring |
| attention `permission`/`error` | "Approval"/"Error" (orange / red) | pill | icon + bar glow | ring + inner glow |

A sidebar row is three quiet lines, modelled on t3code's thread rows:
name with the status slot on the right, the latest message (or the
verb-tense run state), then branch or folder with the agents on the
right. The slot says one thing — attention outranks lifecycle — and
shows the relative time when nothing needs saying. Colour is spent only
there; the name, headline and meta step down in weight and opacity.
Rows at warning severity or above sort first and sit under a "Needs
you" label with a hairline below; with nothing waiting there are no
section headers. The header's attention count is a pill tinted by the
worst attention it counts, in the same colours.

Rules that keep it calm:

- Widgets are reconciled in place (rows keyed by workspace id, tab
  pages by tab id), never rebuilt on refresh, so selection and scroll
  survive events and state changes can transition.
- Rings are inset shadows: attention never changes layout, so it never
  resizes a terminal, and split panes (which clip each child) can't cut
  them off.
- Motion is limited to colour/opacity/shadow on persistent widgets,
  150–200ms ease-out, plus crossfades on reveal. Nothing animates in
  response to keyboard navigation; GTK disables all of it when the
  desktop turns animations off.
- In multi-pane tabs the focused card is outlined and the others'
  status recedes; terminal text is never dimmed.
- Icons the GUI depends on are bundled (`data/icons`), not assumed
  from the icon theme.

Starting an agent in a project — the central action — goes through
the New Workspace dialog (`crates/signaltty-gui/src/new_workspace.rs`,
`AdwAlertDialog` with `AdwActionRow`/`AdwEntryRow`/`AdwComboRow`):
folder (via `GtkFileDialog`, defaulting to the active workspace's
folder or home), name (prefilled from the folder, editable) and
first command (Shell plus every agent whose binary is on `PATH`,
binaries taken from the adapter registry, plus a custom command
field). Enter confirms, Esc cancels, Create is suggested. Confirming
runs `workspace.create` + `pane.spawn` like `signaltty new`, shows
the workspace and focuses the new pane. The dialog is single
instance and shared by Ctrl+Shift+N, the sidebar "+" and the empty
state; the custom field toggles with plain visibility so nothing
animates on keyboard navigation.

Each sidebar row has a close button for its workspace; it shares the
status slot and crossfades in over it on hover or keyboard focus. The
primary menu's Close Workspace action targets the active workspace. Both show a single
destructive confirmation that names the workspace and warns that its running
terminals and agents will stop. Confirming calls the existing
`workspace.close` method and refreshes the full workspace list. Cancel sends
no request. A failed close leaves the row visible and shows a toast.

Dividers persist: each `GtkPaned` reports to `tab.set_ratio` with its
tree path once the drag rests 300 ms, never mid-drag. Reconciliation
compares layout structure ignoring ratios, so a ratio-only echo moves
the live divider in place instead of rebuilding the terminals — no
event → rebuild → event loop. A divider with an unsent drag always
wins over an incoming echo until its own send lands.

## Event refresh and measurement

The GUI merges workspace invalidations into one batch every 16 ms while
there is pending work. Each batch reads each affected workspace once.
Sidebar summaries, the attention count and active tabs share the cached
`workspace.get` result; selecting a workspace does not fetch it again.
Initial load, reconnection and workspace creation/deletion read the full
list. Individual IPC calls still wait synchronously for the actor.

Events route by `workspace_id`, nested `workspace.id`, `pane.workspace_id`,
`tab.workspace_id`, or `notification.workspace_id`. Cached pane/tab indexes
resolve ID-only lifecycle, attention and deletion events, including events
for inactive workspaces. An unknown pane requires at most one `pane.get`
per batch to find its workspace. PTY data goes straight to its terminal.
`maybe_notify` runs for every attention event before batching.

Set `SIGNALTTY_REFRESH_METRICS=1` to log cumulative IPC and server-event
counters to stderr. Each line contains the method/event name, without
payloads. To reproduce the measurement with a separate socket, state
directory, plugin directory and D-Bus session:

```sh
cargo build --workspace
python3 scripts/bench-gui-refresh.py --check
```

The script creates eight workspaces, sends 200 consecutive CLI calls to an
inactive pane and counts GUI IPC after initial mapping/resizing settles.
It stops the isolated server, its panes and the GUI afterward. Use `--gui
/path/to/baseline-gui` without `--check` to measure a baseline built with
the same counters.

Measured on 2026-09-28, excluding startup and notification lookups:

| 200 CLI hook calls | Server events | Before: list / get | After: list / get | Refresh IPC per event, before / after |
|---|---:|---:|---:|---:|
| `PreToolUse` | 1 | 1 / 9 | 0 / 1 | 10 / 1 |
| `PreToolUse --message "tool N"` | 201 | 201 / 1809 | 0 / 33 | 10 / 0.1642 |

Repeated plain `PreToolUse` calls produce only one `agent.working` event:
the server suppresses unchanged lifecycle transitions. Adding a message
produces 200 `notification.created` events and one `attention.created`.
That case makes one additional notification `pane.get` both before and
after: total IPC falls from 2011 to 34. The number of batches depends on
CLI execution speed; the invariant is one workspace read per dirty
workspace per batch. No server or protocol changes are needed.

Cache and routing tests run with `cargo test --workspace`. The GTK test
also exercises batching through `App::on_event`, checks the actual
sidebar labels, attention count, tab indicators and selection, and counts
the separate notification lookups. It requires a display and runs explicitly:

```sh
dbus-run-session -- cargo test -p signaltty-gui event_batches -- --ignored --test-threads=1
dbus-run-session -- cargo test -p signaltty-gui dialog_confirm -- --ignored --test-threads=1
cargo clippy -p signaltty-gui --all-targets -- -D warnings
```
