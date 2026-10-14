# Top bar UI contract
Leading desktop group: Sidebar and New Tab. Center title lane: existing named Workspace Details button. A separate leading blank native drag space reserves 24px, 12px compact. Trailing group: attention, Board/Changes, Main Menu and toolkit window controls. Logical order mirrors for RTL. A hidden counter reserves no width.

Toolbar mode follows content width: below 860sp New Tab/Board/Changes use icons; below 520sp they move to labeled sidebar footer controls and secondary context/decorative mark hide. Window thresholds 1100sp/760sp retain panel docking. The footer remains available when the sidebar is collapsed even if hidden content has not been reallocated. Existing action/menu/shortcut paths and complete Details metadata remain available; Changes toggle state stays synchronized.

No control action or terminal identity changes. Keyboard follows visible functional order, Details remains a native popover, and window chrome remains libadwaita-owned.

Full-width identity remains compact: identity widgets do not expand. Native header layout owns the surrounding free space. The Details hit/focus region wraps mark, name and context rather than occupying the empty header. Identity gaps remain bounded across RTL and wide resizes; compact title allocation and native actions remain unchanged.

The identity lane is the native HeaderBar center title widget. At wide widths its compact Details group stays within 24px of the content-header midpoint. Navigation remains at the leading edge. This supersedes the previous creation-to-Details proximity requirement.

Spacious content uses native strict centering; both compact content breakpoints select loose centering below 860sp. The drag reservation sits outside the title lane so it cannot displace or truncate the centered identity group.
