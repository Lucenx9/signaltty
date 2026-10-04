# Workspace structure

## Sub-features

Pane parentage, exact layout membership, split/close, atomic failed spawn,
sidebar reading hierarchy and pane focus/attention chrome.

## How to get to it (user POV)

Create a workspace/tab, split with Ctrl+Shift+E or Ctrl+Shift+O and close a pane
with Ctrl+Shift+W. Workspace close presents a destructive confirmation.

## Driving it with real IPC/GTK

Run `python3 scripts/qa-server-edge-cases.py`. Foreign tabs, omitted/duplicate
leaves and failed executables must be rejected without corrupting ownership;
finite divider ratios clamp to 0.05–0.95. Run the graphical helper to split/close
a recovered session. `close_workspace_confirms` verifies the actual confirmation.

Run `scripts/qa-ui-scenes.py --output /tmp/signaltty-ui.png` in light and dark,
then with `--width 360 --long-choice`, `--font 'Sans 18'` and
`ADW_DEBUG_HIGH_CONTRAST=1`. Inspect that sidebar messages/metadata span the row,
urgency remains beside Close, inactive pane titles stay readable, and the focused
header stays distinct inside an attention ring. Workspace context is branch-first;
the title tooltip retains the full directory. The header separator and focus
paint must not change terminal allocation during navigation.

## Gotchas

Layout replacement rearranges every owned pane exactly once; removal is explicit
pane.close. Server integration tests also inject legacy hidden panes and prove
closure cleans them. Never close the user's workspace during a verification run.
