# Presentation model
No new domain or persisted entities. Existing workspace identity, branch/agent/directory context, attention count, current view toggle and retained terminal widgets supply the same information. Native HeaderBar regions and existing breakpoints determine layout. The decorative mark gets a parent visibility wrapper so workspace refresh and breakpoint restoration do not compete over the same property.

Window breakpoints own panel docking, while the native content BreakpointBin owns toolbar presentation. Compact footer visibility is derived from sidebar collapse or hidden header New Tab through one weak-reference callback. Attention visibility is derived directly from the existing pane-attention set.
