# Top bar UI contract
Leading desktop group: Sidebar and New Tab. Title lane: existing named Workspace Details button plus blank native drag space (24px, 12px compact). Trailing group: attention, Board/Changes, Main Menu and toolkit window controls. Logical order mirrors for RTL. A hidden counter reserves no width.

Toolbar mode follows content width: below 860sp New Tab/Board/Changes use icons; below 520sp they move to labeled sidebar footer controls and secondary context/decorative mark hide. Window thresholds 1100sp/760sp retain panel docking. The footer remains available when the sidebar is collapsed even if hidden content has not been reallocated. Existing action/menu/shortcut paths and complete Details metadata remain available; Changes toggle state stays synchronized.

No control action or terminal identity changes. Keyboard follows visible functional order, Details remains a native popover, and window chrome remains libadwaita-owned.
