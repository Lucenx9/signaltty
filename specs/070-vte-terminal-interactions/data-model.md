# Runtime terminal state

The server continues to own pane identity, process lifetime, PTY size and output offsets. No persistent model or socket schema changes are planned.

Terminal input is a byte slice validated against the native signal's available C string and declared byte count. The terminating NUL can itself be the control-key payload. If the declared count exceeds the available string storage, the message is already truncated upstream and is refused. The GUI encodes accepted bytes during the callback and forwards them through the existing `pane.input` boundary without UTF-8 conversion.

The existing VTE widget owns selection, paste modes, retained scrollback and text scale. New terminal controls operate on that same widget. A reconnect updates its current screen while retaining its existing history. Output overlap remains the IPC actor's responsibility.

Clipboard reads belong to the pane's current live period. Exit invalidates all
pending reads, and each requested paste within that period remains independent.
Clipboard completion holds a weak pane reference and feeds accepted text through
VTE's paste path.

Text scale starts at normal size for each new pane. It is bounded runtime state and survives a theme restyle in the same pane. It does not change desktop settings or persist between GUI launches.
