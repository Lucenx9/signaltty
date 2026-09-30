# IPC additions

Existing hook-event gets optional waiting-native-permission semantics. When requested with a supported PermissionRequest, register before publish and return a native verdict result only after a valid decision.answer. Bound wait to120s, cancellation returns no verdict. Dedicated connection is single-flight; EOF/shutdown cancels. Existing reporting defaults retain immediate response. Typed params and unknown-field tolerance apply.

worktree.list {workspace_id} -> {worktrees:[{path,branch,head,main,locked,workspace_id?}]}
worktree.create {workspace_id,path,branch,name?} -> {workspace,...}
worktree.open {workspace_id,path,name?} -> {workspace,...}, reusing canonical checkout
worktree.remove {workspace_id,path} -> {removed:true,...}, no force/branch delete

workspace_id accepts id or handle. All new names registered in schema constants. Errors use BAD_PARAMS for invalid/dirty/main/locked targets, PANES_ALIVE for live references and existing IO_ERROR/NO_SUCH_WORKSPACE codes. New methods have real-Git IPC tests. Events pair workspace changes and worktree lifecycle under Store transitions.
