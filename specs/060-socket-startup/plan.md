# Plan: Socket startup preservation

Keep the correction in `signaltty-server::server::serve`, the boundary that
owns socket inspection and removal. Inspect with `symlink_metadata` and the
Unix file type before connecting. Refuse non-socket entries, report inspection
errors, and unlink only after a refused connection.

Use the existing server test context for fast failing startup tests, including
file bytes and an inaccessible live listener's inode. Add symbolic-link and
live-listener refusals, then a real IPC recovery test for an abandoned socket.
Run targeted tests before and after the fix, followed by `scripts/verify.sh full`.

The constitution is satisfied by a recorded specification, test-first changes,
the existing production seam, and the full verification gate. This repairs the
existing startup policy; it adds no architecture or protocol decision.
