# Presentation domain

Project is a repository/folder grouping of existing workspaces. Its identity is a
directory, not a display name. Main and linked checkouts share primary_root;
non-Git workspaces use cwd. Workspace remains the server-owned session container;
task lineage is independent of project membership.

ProjectScope is All or one ProjectId and is persisted by the client. ProjectCatalog
is derived from WorkspaceCache, including workspaces whose detail snapshots are
pending. Legacy aliases map checkout roots and surviving workspace IDs to canonical
identity. A valid scope survives refresh; only an authoritative disappearance resets
it. User navigation reveals an excluded target; a filter change does not navigate.

GitInfo adds optional primary_root with a serde default and omission when absent.
Existing root retains per-checkout meaning. Older clients ignore the extra field;
snapshot version remains unchanged. Background enrichment emits workspace.updated
through the same Store transition. No Project CRUD or parallel workspace owner.
