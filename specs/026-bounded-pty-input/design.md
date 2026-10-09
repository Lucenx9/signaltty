# Design choice

## Grounded ownership

IPC validates pane state, then `PtyManager::input_async` binds the original
writer. An owned async gate serializes requests before the blocking pool.
`write_all` owns a standard mutex and a duplicated master descriptor. Close and
exit remove the registry handle, but the in-flight writer keeps its fd alive.
Runtime shutdown waits for blocking jobs. Portable-pty's writer destructor
writes newline/EOF, so destruction can itself block. Its cloned descriptors
share file-status flags; making input nonblocking also affects output reads.

## Alternatives

1. Put a dedicated writer actor behind a channel and abandon a timed-out job.
   This can make the RPC return but cannot interrupt its kernel syscall. The
   thread/fd and shutdown problem remains; closing its fd from another thread
   does not reliably cancel a syscall and risks fd reuse. Reject this shape.
2. Set the native master descriptor nonblocking and own cloned `File` handles.
   Write with bounded readiness polling, lifecycle cancellation and accepted-byte
   accounting. Wrap output reads so `WouldBlock` waits for readiness rather
   than becoming false EOF. Drop only owned file handles, with no destructor I/O.
   Select this shape: cancellation releases the actual operation and resources.

## Module sketch

```text
pty_io
  open(master) → nonblocking writer + readable wrapper
  InputError {code, message, written_bytes}
  write(bytes, deadline, canceled) → accepted count or InputError
  reader.read → read; WouldBlock → poll; HUP/EIO → drain/EOF
PtyWriter
  async queue, writer mutex, canceled flag
PtyManager
  bind original writer → await gate within deadline → bounded blocking write
  close/exit/replacement → cancel bound writer
  shutdown → cancel all remaining writers
IPC
  typed validation; failures include accepted byte count
```

The PTY I/O module hides fd flags, polling and partial writes. OS calls remain
in server. A timeout on the async gate uses the same total five-second deadline
as I/O. Cancellation is checked between bounded poll waits; the actual syscall
is nonblocking. Future cancellation leaves its guard in the running closure.
Existing automatic Enter keeps its atomic check until the next try-only fix.

Sonnet design delegation hit its API rate limit without returning a design;
this comparison is the implementing agent's analysis, not independent approval.
