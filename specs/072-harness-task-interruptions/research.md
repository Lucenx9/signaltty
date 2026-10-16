# Harness orchestration audit, 2026-10-16

## Reproduced defect

`hook_blockers_without_structured_decisions_park_worker_tasks` initially failed
through real IPC: Claude Notification made the pane blocked but task.get still
returned working. The same path accepts legacy Codex PermissionRequest payloads
without a structured decision. Structured decisions already parked tasks; generic
hook lifecycle blockers lacked that transition. The correction reconciles current
blockers, including initial submission, and preserves native decision priority.

## Launch policy checked

Installed Codex and Claude help confirms the automatic flags already used by
execution.rs. Their default same-provider workers inherit a generated parent
prefix; explicit argv and cross-provider launches remain literal. PR #134 adds
Antigravity defaults and bare argv inheritance from automatic supported parents.
An explicit model/effort or other worker option requires explicit automatic flags.
This audit does not broaden permission policy.

Installed Cursor help offers --force/--yolo, --auto-review, --trust and
--approve-mcps as distinct choices. Cursor is not covered by Signaltty's current
automatic preference. Pi help distinguishes project-file trust (--approve) from
tool permission policy; do not invent a bypass flag for it. Shells and custom
commands remain literal. Authentication, hook trust and provider restrictions are
separate from native command confirmations.

## Permission telemetry gaps, not live reproductions

CursorAdapter consumes session/start/tool/stop hooks but no permission event.
OpenCodeAdapter and its managed plugin forward session lifecycle/error events but
no permission/question event. Once a real hook has reached a pane, the current
hook-authority rule silences all screen fallback. Consequently a permission dialog
without a corresponding lifecycle/decision hook can remain unreported until a
later event or the silent-worker watchdog. This is a source-observed integration
gap, not proof of the user's unspecified live stall or a provider event contract.
A targeted provider fixture/recording and versioned contract are needed before
changing hook authority or adding a provider-specific event mapping.

Pi and generic programs such as Grok/Muse/Antigravity use bundled screen fallback.
Their task blockers share the Store path fixed by PR #134. Screen-only startup
still needs observable readiness; a blanket idle fallback is not proof that an
interactive input widget is ready to accept the prompt.

## Proof boundaries

Source and installed help were inspected for Codex, Claude, Cursor, Pi, OpenCode
integration and bundled screen agents. No live user Signaltty server was available
in this environment, no paid model turn was run, and the user had not yet supplied
specific other harness names or screens. Tests prove Signaltty's IPC/PTY behavior
with synthetic hooks, not every installed provider's current output or event API.


## Manual provider answers without structured telemetry

The existing `tool_hooks_start_work_but_never_clear_blocked` regression preserves
blocked panes across tool hooks, even without a structured decision. Subagent
traffic can overlap an unanswered main prompt, so this correction retains that
guard. An explicit UserPromptSubmit resumes a hook interruption; Stop refreshes
its turn-end evidence. A manual answer in a provider TUI followed only by tool
hooks can therefore leave an unstructured interruption visible during active
work. Reliable early resumption needs attributable approval/turn telemetry. This
is an audit finding, not a behavior this patch claims to solve.
