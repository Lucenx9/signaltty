# Changes and diff reading polish

## Intent and scope
Native Linux signaltty targets coding-agent workspaces with T3 Code visual quality and native GTK interaction. The user authorized the next PR focused on Changes and its diff reader after a render audit. Base 2d5b4cc; branch gui/changes-reading-polish. Task Board and shell title positioning are outside this feature.

## User stories
US1: As a reviewer I can identify Changes and understand whether I compare the whole working tree or the latest agent turn without relying on hover text.
US2: As a reviewer I can scan hunk headings, old/new numbers and code without losing native selection, copying or keyboard navigation.

## Functional requirements
- **FR-001**: Changes retains a visible stable native panel title. Scope selection exposes Working tree and Latest turn as visible labels in a dedicated row, including narrow/enlarged-text/RTL states. The existing head/turn requests, selected state and no-turn guidance remain.
- **FR-002**: Old/new line numbers are visually secondary to code, using the same monospace metrics and semantic foreground. High contrast keeps them fully readable. Hunk headings have clear native paragraph separation and a restrained background. Added/removed signs remain explicit; clipboard text and literal content remain unchanged.
- **FR-003**: The reader remains one native selectable noneditable TextView with internal scrolling. Code/line-number ordering stays LTR within an RTL panel. Narrow windows, long file paths/code, enlarged text, light/dark/high contrast and keyboard focus stay usable.
- **FR-004**: Back restores file selection/focus/list scroll. Refresh/scope/workspace transitions reject stale replies and preserve terminal widget identity. Binary/empty/unavailable/incomplete notices remain explicit. No server/API/dependency changes.

## Success criteria
Visible scope labels fit at 360px with Sans 18 and retain native toggle selection. Gutter remains aligned, hunk-to-code separation improves over baseline, full-copy matches the original numbered diff, and actual GTK scenes are inspected in light/dark/narrow/RTL/high contrast. Existing interaction tests and the repository full gate pass. Independent Spec/Standards reviews are complete before publishing.

## Clarifications and constraints
No unresolved clarification. Preserve the established neutral Signal theme and semantic diff inks; no decorative motion. Real compositor/screen-reader speech/OSK/IME remain manual limits, explicitly recorded. This is presentation using established toolkit primitives, not a new load-bearing architecture decision.
