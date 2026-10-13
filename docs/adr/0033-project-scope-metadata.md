# Derive project scope from server-owned repository metadata

The sidebar groups existing workspaces by repository/folder location. Add optional GitInfo.primary_root for the primary registered checkout so linked worktrees share identity while root keeps its per-checkout meaning. The server computes it, and a finite background legacy sweep can upgrade saved workspaces after the listener is ready. The GUI derives a catalog and persists only its selected scope; no independent Project registry is introduced.

Using workspace names or path prefixes would merge unrelated repositories and separate linked checkouts. Resolving every project through client worktree.list requests would introduce partial/offline joins and late scope changes. Cached additive metadata preserves the existing server/client boundary and old snapshot version. Scope affects native row projection, while sessions and global attention keep their full ownership.
