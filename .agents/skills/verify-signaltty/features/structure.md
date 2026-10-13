# Workspace structure

## Sub-features

Pane parentage, exact layout membership, split/close, atomic failed spawn,
sidebar reading hierarchy and pane focus/attention chrome.

## How to get to it (user POV)

Create a workspace/tab, split with Ctrl+Shift+E or Ctrl+Shift+O and close a pane
with Ctrl+Shift+W. Workspace close presents a destructive confirmation. Tab and
pane close also confirm for live processes, including idle shells; exited and
restored terminals and empty tabs close directly.

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


Workspace refinement: `generated_bare_headlines_do_not_repeat_status_but_messages_and_durations_remain`
covers generated versus explicit activity text. The isolated GTK test
`row_refresh_keeps_status_and_metadata_identity_when_headline_changes` covers
metadata icons, empty/child hiding, tooltips and retained widgets. The existing
`shell_empty_states_explain_actions_and_fit_large_text` fixture also captures the
200px workspace rail with enlarged text and asserted application high contrast.

`app::tests::terminal_closures_confirm_live_targets_and_preserve_failed_tabs`
checks Cancel defaults, no IPC on cancellation, original target through workspace
navigation, VTE/tab identity on cancelled and failed closes, and direct exited,
restored and empty closure. `SIGNALTTY_UI_EVIDENCE` captures production-CSS
light/dark confirmations. The full runner executes it in its own display process.

The desktop reliability probe now cancels its live-terminal close, waits for the
confirmation to dismiss, verifies all three panes remain, then confirms and checks
the resulting two panes through real IPC. This path runs in desktop mode, not full.

`terminal::tests::keyboard_focus_renders_an_inactive_panes_controls` also asserts
a logical visual gap between the paired splits and Close, preserving 28px targets.

Compact workspace hierarchy: `sidebar::tests::workspace_previews_follow_selection_without_hiding_attention_or_tasks` checks direct/programmatic selection, refresh identity, unread/attention/blocked previews and retained task chips. `app::tests::shell_empty_states_explain_actions_and_fit_large_text` captures a ten-workspace fixture, RTL sidebar and 200px enlarged-text rail with protected title/status allocations. `app::tests::workspace_header_context_survives_shell_agent_and_empty_transitions` checks branch/agent/basename/Detached HEAD, full tooltip identity and narrow context hiding. `SIGNALTTY_UI_EVIDENCE` saves actual native light/dark/stress scenes.
