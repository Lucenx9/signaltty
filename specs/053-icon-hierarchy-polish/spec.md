# Feature specification: Icon hierarchy polish

**Branch**: `gui/icon-hierarchy-polish`
**Created**: 2026-10-12
**Status**: Accepted
**Input**: Audit icons, visual hierarchy and command placement across the app against web best practices; use apple-design and emil-design-eng; open a PR. User explicitly reiterated position audit.

## User scenarios and testing

### US1 - Recognize controls without guessing (P1)

Users distinguish Changes, Task Board, creating tabs and arranging panes from the symbols and nearby labels.
**Independent test**: Inspect production toolbar/sidebar/details/empty-state symbols and their accessible names in light/dark and high contrast; activate the same controls.
**Acceptance scenarios**:
1. Changes uses the same diff-oriented symbol in the header, compact sidebar and details card, distinct from pane split symbols.
2. Task Board uses a column-oriented board symbol consistently across header/sidebar/empty state.
3. Application-owned navigation icons share a 16px grid and consistent optical stroke weight; metadata renders at the intended native size.
4. Common native close/menu/navigation symbols remain recognizable. Decorative icons do not add redundant spoken names.

### US2 - Find commands at the right scope (P1)

Users find creation beside workspace context, workspace views together, and pane layout actions separate from closing that pane.
**Independent test**: Check visible ordering and activate New Tab, Board and Changes at wide/compact widths; inspect pane split/close controls and actual widget bounds/focus.
**Acceptance scenarios**:
1. New Tab is a labeled control at spacious widths, close to the workspace breadcrumb, and uses the existing action/shortcut. It becomes icon-only at medium/narrow widths.
2. Task Board and Changes remain a coherent workspace-view group near attention and the app menu; settings stay in the pinned sidebar footer.
3. Close Pane is visually separated from the paired Split Right/Split Down controls; hit areas and activation remain intact.
4. At 360px the workspace context, New Tab, menu and window controls fit. Compact labeled Board/Changes remain in the sidebar as introduced by feature052.
5. Worktree refresh/create controls sit in the window's action header near their content; checkout Open/Remove stay with their own rows.

### US3 - Keep secondary information quiet and usable (P2)

Users read names and context first, while secondary glyphs support recognition without competing with attention/status.
**Independent test**: Inspect sidebar metadata and details, copy a path and open Changes; verify enlarged/high-contrast focus and symbol readability.
**Acceptance scenarios**:
1. Folder/branch metadata and details use the same owned symbols; decorative glyphs align with labels and do not dominate them.
2. Details copy action follows the same restrained control shape and retains its clipboard behavior and accessible name.
3. Pure typography, semantic colors, attention hierarchy and existing terminal widget identities remain intact.

### Edge cases

- Missing external icon themes, symbolic inversion in light/dark, enlarged text and native high contrast.
- Long workspace names/paths and narrow windows; no control overflow or lost keyboard focus.
- Right-to-left UI direction; grouping follows logical start/end rather than hardcoded screen coordinates.
- Empty boards, empty projects, restored panes and hover/focus reveal of pane actions.

## Requirements

- **FR-001**: Semantic icon roles MUST be consistent across surfaces and Changes MUST be distinct from pane split.
- **FR-002**: Owned navigation icons MUST retain monochrome theme-aware rendering, coherent weight and the native16px grid.
- **FR-003**: New Tab MUST stay beside workspace context, labeled when space permits, and retain its existing action/shortcut.
- **FR-004**: Workspace views and pane split/close MUST be grouped by target/function with clear separation.
- **FR-005**: Worktree window actions MUST be in the action header; row-specific operations MUST remain next to their checkout.
- **FR-006**: New controls/placement MUST preserve keyboard order, accessible naming, RTL adaptation and existing hit areas.
- **FR-007**: Light/dark, 360px, enlarged/high-contrast and secondary windows MUST retain readable hierarchy and reachable actions.
- **FR-008**: Server state, IPC, persistence, terminal reconcile and safety/retry flows from feature052 MUST remain intact.

## Key entities

No model changes. Existing workspace, tab, pane, checkout and UI icon roles are presentation inputs.

## Success criteria

- Each visible Changes entry uses the same recognizable icon; it differs from both split directions.
- Native icon resolution succeeds without relying on an external icon theme for owned symbols.
- New Tab is visible/labeled at desktop widths, icon-only at constrained widths, and remains inside the360px window.
- Pane close is separated from split actions while every existing action remains operable.
- Worktree actions and owned row targets survive repositioning; details copy still produces the exact full path.
- Independent source/visual review and full verification pass without terminal identity regressions.

## Assumptions

Keep the accepted restrained T3-like direction and native GTK interaction patterns. Use original monochrome SVGs in the existing icon bundle; no icon/font library or copied Apple symbol artwork. Common native system symbols stay native. The audit does not authorize model/IPC changes, toolbar customization, new navigation destinations or decorative icons on every row. This PR is stacked on the still-open #100 so its review diff contains only this polish.
