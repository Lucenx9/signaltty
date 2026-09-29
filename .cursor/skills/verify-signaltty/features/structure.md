# Workspace structure

## Sub-features

Pane parentage, exact layout membership, split/close, atomic failed spawn.

## How to get to it (user POV)

Create a workspace/tab, split with Ctrl+Shift+E or Ctrl+Shift+O and close a pane
with Ctrl+Shift+W. Workspace close presents a destructive confirmation.

## Driving it with real IPC/GTK

Run `python3 scripts/qa-server-edge-cases.py`. Foreign tabs, omitted/duplicate
leaves and failed executables must be rejected without corrupting ownership;
finite divider ratios clamp to 0.05–0.95. Run the graphical helper to split/close
a recovered session. `close_workspace_confirms` verifies the actual confirmation.

## Gotchas

Layout replacement rearranges every owned pane exactly once; removal is explicit
pane.close. Server integration tests also inject legacy hidden panes and prove
closure cleans them. Never close the user's workspace during a verification run.
