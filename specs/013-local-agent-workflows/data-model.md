# Data model

NativePermission is ephemeral: provider, generated decision id, live pane owner, prompt/options and response waiter. Public Decision remains render data. Waiter loss clears that same id only; persisted native decisions cannot retain answerable state after restore. Native verdicts are AllowOnce or Deny; no timeout value represents approval.

Worktree registration comes from Git: canonical path, HEAD, branch, main/bare/locked/prunable flags. Workspace canonical cwd associates it with a registered checkout; no new persisted core fields are needed. Checkout mutation/removal reservations are server runtime state. Git owns branches; workspace close does not delete them.

Palette choice is either an existing window action or workspace identity. Search and zoom are GUI state. Zoom never replaces persisted layout and never removes hidden VTE objects. Diff retains current files/directories/totals contract with binary/untracked distinctions.
