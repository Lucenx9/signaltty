# Feature Specification: Priority-Sorted Sidebar

**Feature Branch**: `001-priority-sorted-sidebar`

**Created**: 2026-09-29

**Status**: Draft

**Input**: User description: "docs/14 directive 1 requires a priority-sorted sidebar (blocked → done → working → idle); the GUI currently lists workspaces in server order"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Most urgent workspace floats to the top (Priority: P1)

A user with several workspaces open glances at the sidebar; the workspace
whose agents need them most (approval waiting, failure, fresh output) is
listed first, without scrolling or hunting.

**Why this priority**: This is the core of directive 1 — the product *is*
the "which agent needs me" loop. Server (creation) order buries urgent
workspaces under older idle ones.

**Independent Test**: Open workspaces in idle → working → approval-waiting
order; the approval-waiting workspace renders first. Fully testable by
`sort_summaries` unit tests plus one display test asserting row order.

**Acceptance Scenarios**:

1. **Given** workspaces A (idle), B (working), C (permission_required),
   **When** the sidebar renders, **Then** the order is C, B, A.
2. ~~**Given** two workspaces with equal attention and lifecycle,
   **When** the sidebar renders, **Then** the one with the most recent
   activity is first~~ — SUPERSEDED by the 2026-09-29 amendment below:
   ties break by workspace age (newer first), then name.
3. **Given** a workspace whose attention clears,
   **When** the next refresh applies, **Then** it sinks to its new
   priority position without losing the current selection.

---

### User Story 2 - Lifecycle order follows the directive (Priority: P2)

Within equal attention severity, lifecycle decides: blocked first, then
failed, then done, then working, then idle (unknown/exited last).

**Why this priority**: `docs/14` §1 names the order explicitly
(blocked → done → working → idle). The existing `urgency()` rank
(working above done) serves roll-ups, not sorting: a finished turn
needs review, while a working agent needs nothing.

**Independent Test**: Unit test on the rank function covering every
lifecycle variant.

**Acceptance Scenarios**:

1. **Given** four panes-free workspaces all with attention none and
   lifecycles working/done/blocked/idle, **When** sorted,
   **Then** the order is blocked, done, working, idle.

---

### Edge Cases

- Empty workspace list: no rows, no crash (existing empty state).
- Workspaces with no panes: attention none, lifecycle unknown → sink
  to the bottom, ordered by name.
- Equal keys on every field: name tiebreak keeps the order
  deterministic across refreshes (no row jitter).
- Selection follows the workspace, not the row index (existing
  reconcile-by-id already guarantees this; no change needed).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The sidebar MUST list workspaces ordered by descending
  attention severity (`Attention::severity`), then descending lifecycle
  sidebar rank, then descending workspace age (`created_at`, newer
  first), then ascending name. (Amended 2026-09-29: `last_activity_at`
  removed — see amendment.)
- **FR-002**: Lifecycle sidebar rank MUST be blocked > failed > done >
  working > idle > unknown/exited (directive 1 order, with failed
  slotted next to blocked as it also needs triage).
- **FR-003**: Sorting MUST be a pure function over sidebar summaries,
  applied in `App::apply_refresh` before `Sidebar::update`, so row
  reconciliation (selection/scroll preservation) is untouched.
- **FR-004**: The rank function MUST live in `signaltty-core` next to
  `urgency()` with in-module unit tests; the sort MUST live in
  `signaltty-gui/src/sidebar.rs` with headless unit tests.

### Key Entities

- **WsSummary**: existing sidebar row model (`id, name, lifecycle,
  attention, message, meta, last_activity`) — sorting key source, no
  shape change. (Amended 2026-09-29: gains `created_at` for the stable
  tiebreak and `disambiguator` for same-name rows — see amendment.)

## Amendment 2026-09-29: stable order + disambiguated rows

Field report (two live "simone" workspaces, both Running): rows with
identical text swapped positions on every agent event and the clicked
workspace never looked settled. Root causes:

1. The `last_activity_at` tiebreak rewrites the order on every hook
   event, notification, and lifecycle transition. Rows reorder under
   the pointer: clicks land on the wrong row and GTK drops the
   selection on every remove+insert move. Fresh output already floats
   via `unread` severity, so the tiebreak's information value never
   justified its interaction cost. Replaced by workspace age (newer
   first): stable forever, still recent-ish.
2. Same-name workspaces render byte-identical rows (name · headline ·
   place). Rows now render `name · handle` — and the header title too —
   whenever sibling names collide; handles are unique by construction.
   Quiet otherwise (Linear-grade restraint).
3. `Sidebar::update` snapshots the selected workspace and restores it
   after reconciliation, since GTK does not preserve selection across
   remove+insert moves and the refresh path only re-selects when the
   active workspace is in the changed set.

## Amendment 2026-10-05: nested task workspaces + context menu

Orchestrator tasks create independent task workspaces named after the
task label. In a flat list, task workspaces mix with user workspaces
chaotically.

1. **Hierarchy & Flattening**: Tasks in `model.tasks` with both a
   `pane_id` and `parent_pane_id` define a child-to-parent workspace
   relationship. Nested parent chains flatten to ONE level (root workspace).
   Orphan children whose parent is absent from the sidebar, or cyclic
   relationships, gracefully fall back to ordinary root workspaces.
2. **Group-Aware Priority Ordering**: Groups (root + its child task
   workspaces) sort by the group's worst attention severity and best
   lifecycle rank, with root `created_at` and `name` as tiebreaks. Each
   root is immediately followed by its children, sorted among themselves
   by the standard sort key.
3. **"Needs you" Section Cohesion**: If any member of a group needs
   human attention, the root and all its children enter the "Needs you"
   section together, preventing disjointed groups.
4. **Row Presentation & Collapsing**:
   - Child rows render in a compact 2-line layout (`workspace-child`)
     indented by ~20px with the mark and 3rd metadata line hidden (place and
     agents move to tooltip).
   - Root rows with children display a disclosure toggle ("▾ N tasks · M needs you"
     / "▸ N tasks") in their metadata line that expands or collapses child
     rows in-memory. Selection of a hidden child auto-expands the group.
5. **Context Menu**: Right-clicking (button 3) or pressing Menu / Shift+F10
   selects the workspace and presents a popover context menu with
   workspace actions (Rename Workspace, Show Changes, Worktrees, New Tab,
   and Close Workspace).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With workspaces in idle/working/approval states, the
  approval workspace renders as the first sidebar row.
- **SC-002**: `cargo test -p signaltty-gui` and
  `cargo test -p signaltty-core` pass, including the new ordering tests.
- **SC-003**: The existing ignored display test
  (`event_batches_keep_sidebar_attention_tabs_and_notifications_consistent`)
  still passes under both light and dark schemes (row reorder must not
  break reconcile/selection).

## Assumptions

- Attention severity order in `signaltty-core` is authoritative and
  unchanged (error > permission > input > warning > unread > none).
- `urgency()` keeps serving worst-lifecycle roll-ups; the new rank
  serves sorting only. Two ranks, two documented purposes.
- No server change: ordering is a client presentation concern, like
  `focus.next_unread` is a server query concern.
