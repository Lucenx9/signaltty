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
overlay below 760sp, resizable via a drag handle on its edge clamped to
200–560px and persisted across restarts), `AdwTabView`/`AdwTabBar` (autohides with one
tab), one primary menu, every command a `win.*` action with an
accelerator. Colour comes only from libadwaita variables, so light,
dark, the system accent and high contrast follow the desktop;
terminals take the desktop monospace font with 1.1× leading, GNOME's
ANSI palette on dark panes and a light palette whose inks read at 4.5:1 on
white apart from two deliberately dim greys (unit-tested), and a pane title
without the shell's `user@host:` prefix when the host is the local one (a
remote SSH host stays visible).
Dark mode redefines those variables (under a `.dark` window class
mirrored from `AdwStyleManager`) into a wider lightness ladder —
sidebar, near-black canvas, pane cards — edged by hairlines, per
docs/14 §7; light mode keeps stock Adwaita values.
Preferences (Ctrl+,) picks an appearance (System / Light / Dark) and one
of five themes (Signal, Grove, Ocean, Ember, Iris); a `theme-<id>` window
class redefines the same variables per theme and variant, Signal keeping
the desktop accent (ADR-0019, `specs/017-themes`). The choice persists in
`$XDG_CONFIG_HOME/signaltty/gui.json`. Each appearance is previewed as a
miniature window (System split light/dark down the middle) and each theme as
a light and a dark orb; selection is an accent ring, never a fill
(`specs/042-preferences-previews`).

### Discoverable navigation

The sidebar shows a search-style launcher below the titlebar. It is a real
button bound to `win.command-palette`, not an editable search entry: clicking
or pressing Enter opens the existing keyboard-first Commands and Workspaces
palette (`Ctrl+Shift+P`). The launcher uses bundled symbolic artwork and
semantic Adwaita surfaces across light, dark, custom themes and high contrast.
New Workspace remains in the sidebar header.

Only the **Workspaces** list scrolls (Adwaita `.caption-heading` title,
live total). Worktrees and Preferences are pinned in the sidebar footer, so
a long list never buries them; they activate the existing `win.*` handlers,
with no second navigation state or fake dashboard behind them. The footer
deliberately does not display an invented "connected" state or agent count.

Task Board (`Ctrl+Shift+B`) and Changes (`Ctrl+Shift+D`) have exactly one
visible entry point: the content header, next to New Tab and the attention
indicator, where Changes keeps its toggle state. Both are
`AdwButtonContent` buttons labeled at spacious desktop widths; below 1100sp
the label is cleared and they take the same flat `.image-button` style as
the other header buttons, keeping their accessible names. Below 760sp they
hide completely; the main menu, the command palette (also reachable from
the sidebar launcher when the overlay sidebar is opened) and the shortcuts
remain. No new IPC, fake panes or full-width second toolbar: terminal
content and workspace context continue to own the canvas.

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
the workspace mark (monogram on a stable per-workspace tint, kept off
the accent and status hues; the content header repeats it in a
left-aligned `mark name / context` breadcrumb) and name with the
status slot on the right — "Working 3m", the age dropped under a
minute — the latest message (or the
verb-tense run state), then branch or folder with the agents on the
right. The slot says one thing — attention outranks lifecycle — and
shows the relative time when nothing needs saying. Colour is spent only
there; the name, headline and meta step down in weight and opacity.
Rows at warning severity or above sort first and sit under a "Needs
you" label with a hairline below; with nothing waiting there are no
section headers. The header's attention count is a pill tinted by the
worst attention it counts, in the same colours. Finished child tasks
soften only their secondary headline: the workspace name, urgency
indicator and keyboard focus stay at full strength. Hover, selection
and high contrast restore that headline to full opacity.

The close target reserves space on the name/status line only. Activity and
the location/agent line span the row's full width, with roomier line spacing
and readable secondary text. Workspace header context shows the agent summary when agents are present.
For shell workspaces it puts the branch before the directory, or shows the
directory alone without a branch. The separator follows context visibility;
startup and the last-workspace removal leave no dangling slash. Hovering the
context exposes the complete branch and path, including when the visible
labels ellipsize in a narrow window.

Rules that keep it calm:

- Widgets are reconciled in place (rows keyed by workspace id, tab
  pages by tab id), never rebuilt on refresh, so selection and scroll
  survive events and state changes can transition.
- Rings are inset shadows: attention never changes layout, so it never
  resizes a terminal, and split panes (which clip each child) can't cut
  them off.
- Pointer presses use a 120ms ease-out transform at scale 0.97.
  Focus, attention styling and control reveal change instantly; keyboard
  presses never scale. Native asynchronous state reveal remains toolkit-owned.
  The window follows GtkSettings animation preferences at startup and on
  changes, removing both interpolation and static press transforms.
  High contrast removes muted sidebar text and pane subtitle opacity, and
  strengthens header separators and the focused header's leading marker.
- Workspace switches never animate pages: bulk tab replacement (close
  cascade + select) runs with the tab view hidden so one paint shows
  the final state. `AdwTabView` slides on programmatic selection and
  terminals smear through it — context jumps stay instant, tab browsing
  inside a workspace keeps the native slide.
- Pane headers have a quiet neutral surface and an inset separator above
  output. In multi-pane tabs the focused card is outlined and its header
  has a subtle accent wash, visible even when attention takes over the ring.
  Inactive titles and terminal text stay readable without extra dimming.
- Icons the GUI depends on are bundled (`data/icons`), not assumed
  from the icon theme.

Starting an agent in a project — the central action — goes through
the New Workspace dialog (`crates/signaltty-gui/src/new_workspace.rs`,
`AdwAlertDialog` with `AdwActionRow`/`AdwEntryRow`/`AdwComboRow`):
folder (via `GtkFileDialog`, defaulting to the active workspace's
folder or home), name (prefilled from the folder, editable) and
first command (Shell plus every agent whose binary is on `PATH`,
binaries taken from the adapter registry and the bundled generic
screen manifests, plus a custom command field). Enter confirms, Esc cancels, Create is suggested. Confirming
runs `workspace.create` + `pane.spawn` like `signaltty new`, shows
the workspace and focuses the new pane. The dialog is single
instance and shared by Ctrl+Shift+N, the sidebar "+" and the empty
state; the custom field toggles with plain visibility so nothing
animates on keyboard navigation.

Each sidebar row has a separate trailing close target, revealed instantly
on hover or keyboard focus while the status remains visible. Its reserved
space keeps text stationary. Keyboard focus outlines are inset, and pane
actions reveal on focus anywhere in the pane. The
primary menu's Close Workspace action targets the active workspace. Both show a single
destructive confirmation that names the workspace and warns that its running
terminals and agents will stop. Confirming calls the existing
`workspace.close` method and refreshes the full workspace list. Cancel sends
no request. A failed close leaves the row visible and shows a toast.

Inline approvals place the complete selectable, wrapped question above
native `AdwWrapBox` choices. Long option labels wrap as well, so a 320px
pane never forces the split wider. Choices keep their original decision
and option IDs; removing the decision bar retains the mounted VTE.
Header controls and workspace rows expose descriptive accessible names.
Choice buttons follow the current decision ID, answerability and options,
including when those options change under the same ID. Losing the answer
channel removes the choices and shows the existing terminal-answer hint.
Prompt-only updates keep mounted choice buttons and the VTE.

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
list. IPC replies are awaited on GTK's local executor, with no blocking
main-thread receive. A refresh uses an owned cache copy behind a serial
async lock, so no RefCell borrow crosses an await. Invalidations arriving
in flight form the next batch. Socket operations and ordinary requests have three-second
deadlines; uncertain mutations are reported and never automatically replayed.

Attach is enqueue-only. Initial and reconnect VT snapshots replace the existing
terminal screen through the same UI FIFO as live bytes. A cumulative output
offset suppresses bytes already covered by the snapshot, including queued
output from before attachment. Repeated application activation presents the
existing window. Current lifecycle presentation derives from process live state;
restored historical working state cannot animate a stopped process.

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

Direct supported agent launches prepare status hooks before execution. Setup
errors, disabled launch modes and newly configured Codex trust requirements
use native `AdwToast` notices on create, split and resume; successful ordinary
configuration does not add another status badge (ADR-0013).


## Local navigation and Git workflows

The primary menu and command/workspace palette share the action registry.
Ctrl+Shift+P opens the palette. It filters commands and cached workspace names
and paths immediately, so typing and pressing Enter selects the current match.
Up/Down selects a result and scrolls just enough to keep it visible while search retains
focus. Filtering resets to the first visible match. No matches shows a native status
page with recovery guidance; clearing the query restores the list. Commands show
GTK-formatted shortcuts below their titles, including accessible descriptions, without
narrowing the title at large text sizes. Full workspace names and paths remain available
as literal tooltips. Enter invokes the existing action or switches the
workspace. Escape restores terminal focus. Workspace switches keep already
created VTE widgets attached to the output stream, including inactive workspaces;
closing a pane or workspace prunes those widgets after the cache refresh.

Ctrl+Shift+R opens Rename Workspace through the existing `workspace.rename`
method. Empty names disable Rename; labels change while handles stay fixed.
Ctrl+Shift+F opens literal search inside the focused pane. Enter/Shift+Enter and
the next/previous buttons navigate VTE scrollback matches. Escape closes search,
clears its regex and selection, and restores terminal focus. Search never sends
text to the process. Its controls fit a 320px pane.

Ctrl+Shift+Z temporarily shows the focused pane alone. Zoom projects the cached
layout locally and retains every pane widget, including hidden siblings and
their output. It never persists a divider change. Returning to the split uses
the latest server layout and ratios; switching workspace/tab or focusing another
pane restores the ordinary view. Hidden widgets do not send resize requests.

Worktrees opens a native registered-checkout list with branch and open-workspace
context. Create asks for an absolute path and a new branch; Open reuses an
existing workspace when present. A newly opened empty workspace starts a shell
through `pane.spawn`. Remove requires a separate destructive confirmation and
keeps the branch. Open, main, locked, missing and bare checkouts cannot be
removed from this dialog; the server also refuses dirty or live-referenced paths.
Closing a workspace leaves its checkout intact. Git operations use a dedicated
background IPC connection with a 90-second deadline, so approvals and normal
control traffic keep flowing. A checkout's Open and Remove actions sit on
one aligned row. Remove is a quiet destructive-text action, never a second
primary button. Disabled controls have contextual accessible descriptions
for main, locked, missing, bare and still-open checkout states. An uncertain
mutation is never automatically replayed.

Show Changes (Ctrl+Shift+D, header toggle, palette, main menu) docks a panel at
the window's right edge, beside the terminals, in an end-side
`AdwOverlaySplitView`. Below 1100sp it overlays the content; below 760sp the
header toggle hides so the header fits 360px. The panel reads `workspace.diff`
when opened, when the active workspace changes while it is open, and on explicit
Refresh; a closed panel does no reads. Its summary says "Changes against HEAD"
with aggregate file/line counts. Files sort by path under a directory label per
group; each row shows the file name (full path in the tooltip) and `+N −N` in the
diff colours, or "Untracked"/"Binary". It does not imply a per-turn diff.
Workspace names, paths and file names render literally, including `<`, `>` and `&`.

Clicking the header breadcrumb opens the workspace details card
(`details.rs`, after t3code's thread details): branch (or "Detached HEAD"),
the `~` path with Copy Path, running agents, the workspace's task with its pull
request and check status, `Changes +N −N` (read with `workspace.diff` only when
the card opens; it opens the Changes panel), and Open Folder. Rows that do not
apply are hidden; the breadcrumb is inactive without a workspace.

Activating a file pushes a reader inside the same panel, using
`workspace.file_diff`. Back returns to the preserved file list and row focus.
The selectable, noneditable monospace view shows hunk headings, old/new line
numbers, context, and explicit added/removed signs. Semantic text colors support
light and dark themes; long code lines scroll within the reader. New text files
show additions. Binary, unchanged, unavailable and incomplete previews have
explicit notices. Refresh invalidates the old patch and reads the current
summary. The reader ignores replies superseded by selection, Back, refresh or a
workspace switch. File reads use a dedicated connection with a ten-second deadline
and never alter terminal widgets. See [ADR-0016](adr/0016-file-diff-review.md).

The display tests `navigation_palette_fast_enter_and_git_dialogs_use_native_controls`,
`pane_zoom_keeps_hidden_terminals_and_restores_latest_ratios`, and
`terminal_search_is_literal_and_keeps_the_terminal` exercise the actual controls.
Set `SIGNALTTY_UI_EVIDENCE=/tmp/signaltty-local-workflows` on the first two to
export light/dark palette, Git, search and zoom snapshots, including narrow Git
dialogs. The fixtures use independent GTK windows and IPC channels.

The isolated display tests
`palette_keyboard_selection_stays_visible_while_search_keeps_focus` and
`palette_empty_results_and_shortcuts_fit_narrow_appearances` cover long-list navigation,
filtering after scrolling, no-match recovery and 360px layouts. `SIGNALTTY_UI_EVIDENCE`
exports their native renders, including enlarged text with the application's
high-contrast class. This class-based scene does not certify a screen-reader or a real
desktop high-contrast theme.

## Task Board presentation

The Task Board retains its five lifecycle columns and native vertical lists.
Each column uses a distinct heading and quiet numeric badge instead of a
punctuated heading/count string. Only a nonempty Needs you counter takes a
semantic warning tint; high contrast keeps counters outlined and readable.
A horizontal GTK scroller keeps every column reachable in narrow windows,
including by keyboard focus. Cards keep complete tooltips while labels
ellipsize. Secondary metadata uses a single foreground treatment; high
contrast restores metadata and column headings to full strength. Task
classification and pane activation remain unchanged. Done keeps its total count
and newest 20 cards; when more exist, a wrapping “Showing latest 20 of N”
notice explains the visible subset.

Background task events reconcile the open board in place. Task IDs retain row
widgets, current metadata and pane destinations. Scrolled columns keep their
surviving viewport anchors through insertions and removals; a moved focused card follows its
identity into its new column. Removal chooses the next/previous surviving row
in the original column, then native dialog focus. New navigation wins over
pending restoration; GTK layout clamps and board-triggered focus animations
do not count as new user scrolling. Empty/nonempty transitions retain the dialog.

The isolated display test
`task_board_fits_narrow_windows_and_reveals_last_column` checks narrow
light/dark scenes, enlarged high-contrast text, scrolling to Done and
activating its pane. `SIGNALTTY_UI_EVIDENCE` exports the native renders.
