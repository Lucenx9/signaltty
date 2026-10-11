# GUI-local state

No persisted core model changes. Existing Pane.live and Pane.restore_state determine whether stopping a real session requires confirmation.

Workspace draft retains folder, name/automatic name, command and optional existing workspace id. Ready -> Pending -> Complete or Failed. Failed before creation keeps all fields editable; Failed after creation stores its id and freezes workspace fields while command remains editable. Both retry explicitly. Pending cannot submit or dismiss twice. Closed callbacks re-arm the single-instance action.

Close confirmations capture stable pane/tab id and label. Cancellation never invokes IPC. Reconcile-owned page removals bypass confirmation and finish the native page close.
