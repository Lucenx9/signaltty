# Feature specification: Sidebar app identity

Branch `gui/sidebar-identity-polish` | Created 2026-10-14 | Status Ready for planning

Input: Improve the left area labeled signaltty, including the app icon if useful; research web best practices, use Emil/Apple/frontend-design skills and deliver a PR.

## User scenarios and testing

### US1 Recognize the app without competing with workspaces, P1
The app identity has a small original mark and calm wordmark, aligned with the sidebar's controls. It stays static and does not resemble a new action or project selector.
Independent test: native light/dark, sidebar at its 200px minimum, enlarged text, native high contrast and RTL renders.
Acceptance: full app name and mark are visible at ordinary sidebar widths; at narrow/enlarged text the name can ellipsize with full tooltip while New Workspace stays visible and usable. Keyboard skips decorative branding and reaches the creation action.

### US2 Recognize the same identity on the desktop, P1
Launcher and symbolic icons express the same original prompt/parallel-line metaphor. Small versions remain legible and bundled resources resolve without an external icon theme.
Independent test: resolve regular and symbolic resources with empty filesystem theme paths, render regular icons at 16/32/64/128 and symbolic versions at 16/24, and run isolated desktop installation tests.
Acceptance: UI uses the symbolic variant following native text colors; launcher/notifications/About keep the same application ID and icon name. Installation delivers the artwork bundled in the binary.

## Requirements
- FR-001: Replace the bare bold title with a restrained original app mark and wordmark, using native GTK components and the system font.
- FR-002: Use one recognizable prompt/parallel-work metaphor for regular and symbolic app icons. Preserve application/resource/desktop-entry IDs and existing installation paths.
- FR-003: Keep branding non-interactive and decorative artwork out of the accessibility focus order. Preserve readable app-name text, full tooltip and New Workspace's named action, tooltip and accelerator.
- FR-004: Retain visible controls, bounded allocations and logical mirrored alignment at 200px sidebar width, enlarged text and RTL. Native high-contrast styling applies to the symbolic mark.
- FR-005: Original self-contained SVG artwork; no external fonts, icon-library dependency, new animation, protocol, persisted state or terminal rebuilding.
- FR-006: Native renders, installation verification, complete full gate and independent code/spec/visual review precede PR delivery.

## Success criteria
- SC-001: Baseline/after real GTK scenes show a coherent small brand lockup without adding header height or a second toolbar.
- SC-002: Branding and New Workspace do not overlap or leave the sidebar in narrow/enlarged/RTL fixtures; New Workspace remains operable from the keyboard and pointer.
- SC-003: Regular and symbolic app icons resolve from bundled resources without filesystem themes and render cleanly at the tested small/large sizes.
- SC-004: Full gate and two independent review axes pass; manual compositor/assistive technology limits are explicit.

## Assumptions and scope
Base is `f8c3f18`, head of open parent PR #106. Deliver a stacked PR against gui/top-bar-polish unless the parent merges before publication. No merge authorization requested for this task. The mark's metaphor is a design choice within the user's authorized scope. Native accessible names/focus and high contrast are testable here; actual desktop launcher/switcher presentation, screen-reader speech/order, OSK, IME and compositor drag are not certified by Xvfb. The user authorized replacing app artwork; no product rename or broad theme redesign is implied.
