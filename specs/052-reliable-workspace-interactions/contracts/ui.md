# UI contracts

- Cancel is default for stopping running sessions; Esc cancels; explicit Close Terminal/Close Tab confirms the original target. Pending or failed closes do not optimistically remove terminal widgets.
- New Workspace Create/Enter is enabled only for valid draft and no pending attempt. Pending shows progress and blocks dismissal; failure preserves fields and exposes Retry with inline text. Existing partial workspace is reused on retry. Dismissal after partial failure keeps that server-owned workspace and states that it was created.
- Labeled compact Board/Changes controls live outside the scrolling workspace list, appear only when header tools are hidden, and activate existing actions/state. Menu, palette and accelerators remain unchanged.
- No IPC method/schema changes.
