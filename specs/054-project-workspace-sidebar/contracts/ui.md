# UI and metadata contract

The project selector is pinned below Search. Native choices expose All projects,
project name, full path, workspace count and a selection check. Paths distinguish
same-named projects. Search matches names and paths; no matches shows an explicit
message. Enter activates a matching choice; Down moves focus into the list. Escape
closes the popover without changing scope and returns focus to the trigger.

All workspace rows remain in memory. Scope changes their visibility and priority
order without destroying rows or terminals. An included child whose parent is
outside the scope appears as a root. Expanding a group cannot reveal excluded
workspaces; collapse survives scope changes. Global attention and the palette stay
unfiltered; explicit navigation reveals their target. Background refresh preserves
valid scope. New Workspace proposes the selected folder while retaining the editable
form, pending-close guard and retry behavior.

Optional GitInfo.primary_root comes from the first NUL-delimited worktree record.
GitInfo.root retains its current-checkout meaning. Missing metadata defaults safely
and receives finite asynchronous backfill. No new IPC method. Scope persistence
uses client project-scope.json independently of gui.json; missing projects reset
only after a complete live catalog. Metadata upgrades migrate unambiguous legacy
checkout scope keys.
