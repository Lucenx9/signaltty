# Design decisions

## References
T3 Code local checkout `/home/simone/code/t3code`, commit `740bda40c9`: `apps/web/src/index.css` compact geometry, `components/sidebar/SidebarThreadHeader.tsx` restrained header grouping, `components/Sidebar.tsx` primary names and secondary context.
[GNOME header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html) recommends grouping primary actions, adapting to narrow widths and tooltips.
[GNOME principles](https://developer.gnome.org/hig/principles.html) recommends progressive disclosure and nearby frequent actions.
[Apple sidebars](https://developer.apple.com/design/human-interface-guidelines/sidebars) supplies hierarchy guidance; Apple spatial consistency and immediate feedback are translated to existing GTK controls.
Requested skills: frontend-design, emil-design-eng, apple-design. Engineering: karpathy-guidelines; verification: verify-signaltty; independent review: code-review.

## Decision: Compact identity, progressive activity
The workspace name and state share a row. Root location/disclosure remain below, allowing similarly named branches to stay distinguishable. Explicit previews appear for selected or attention-bearing rows; task chips remain available on their own line. Alternative considered: hiding all metadata and chips until selection. Rejected because it weakens triage and discoverability. Alternative: reduce font size globally. Rejected because enlarged text must remain supported.

## Decision: Short header context, full details
Branch or Detached HEAD comes first even in agent workspaces. Otherwise agents or the directory name supply concise context. Full path remains in tooltip, accessible button identity and Details card. Alternative: show branch plus path at all times. Rejected because the path competes with workspace title and actions.

## Tokens and critique
Retain Signal light/dark canvas/surface/border/text/accent roles, existing system font and 6px row/control geometry; no copied palette or new font. Primary name retains semibold; metadata uses existing secondary foreground; context uses a smaller system-font step. Preserve 16px metadata icons and native close hit areas. The deliberate visual choice is a calm two-line navigation row, with task/attention content taking additional space only when useful. Review rejects a generic new card treatment: use the existing neutral selection surface and leading marker.

## Native capture correction
A rigid primary row reduced a generated name to an ellipsis at 200px/Sans 18. Use the already-supported [AdwWrapBox](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.7/class.WrapBox.html), whose natural-size wrapping moves status below identity when they cannot share a line. No resize callbacks, new persisted state or custom widget. A native geometry assertion reserves at least 80px for the title at the tested minimum rail width. The header's existing overlay breakpoint hides a parent context container, preserving full metadata in Details and accessible text without competing visibility writers.
