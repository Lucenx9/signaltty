# UI contracts

Changes glyph is shared across header, compact footer and Details. Board glyph is shared across header, compact footer and empty board. Owned symbols resolve from the resource bundle and render monochrome in both appearances.

New Tab stays in the active workspace header's start cluster after its breadcrumb; its label clears under1100sp. Workspace view controls remain trailing near menu and attention, replacing with labeled compact footer controls below760sp. Logical start/end placement mirrors with RTL. All existing actions, hit targets, focus semantics and safe closure/recovery remain intact.

Worktree Refresh/Create are header actions, Open/Remove remain checkout-row actions. Their native icon/label content and Create can shrink within narrow, enlarged-text allocations while accessible names retain their full meaning. Details icons are decorative, Copy Path remains named and copies the full path. No external API changes.
