# Top bar UI contract
Leading desktop group: Sidebar and New Tab. Title lane: existing named Workspace Details button plus blank native drag space (24px, 12px compact). Trailing group: attention, Board/Changes, Main Menu and toolkit window controls. Logical order mirrors for RTL. A hidden counter reserves no width.

Toolbar mode follows content width: below 860sp New Tab/Board/Changes use icons; below 520sp they move to labeled sidebar footer controls and secondary context/decorative mark hide. Window thresholds 1100sp/760sp retain panel docking. The footer remains available when the sidebar is collapsed even if hidden content has not been reallocated. Existing action/menu/shortcut paths and complete Details metadata remain available; Changes toggle state stays synchronized.

No control action or terminal identity changes. Keyboard follows visible functional order, Details remains a native popover, and window chrome remains libadwaita-owned.

Full-width identity remains compact: only blank drag space expands. The Details hit/focus region wraps mark, name and context rather than occupying the empty header. Identity gaps remain bounded across RTL and wide resizes; compact title allocation and native actions remain unchanged.

The identity lane is packed directly after navigation in the native leading HeaderBar region. The creation-to-Details gap stays within 24px; the center widget is blank.
