# Implementation Plan: Native workspace UI refinement

## Grounding

Server snapshots flow through the async actor/cache into persistent workspace
rows and mounted pane widgets. PaneWidget owns approval controls and VTE; App
owns async actions, focus/navigation guards and desktop preference subscriptions.
CSS is GTK CSS loaded from GResources, not browser CSS. GTK settings are the
source for animation preferences on the declared GTK 4.18 baseline.

## Design

Preserve the existing three-line sidebar and separate the trailing close action
from urgency. Put the complete wrapped question above one native AdwWrapBox of
wrapped choice labels. It keeps short choices compact and uses the existing
libadwaita 1.7 dependency. No duplicate responsive trees or custom measurement.

Keep focus, focus-driven control reveal and frequent page navigation immediate.
Pointer presses use the existing 120ms strong ease-out and a subtle 0.97 scale;
keyboard-visible controls do not scale. Existing native asynchronous status
fades remain toolkit-owned. App mirrors animation and high-contrast preferences
into root classes at launch and on notification, with weak captures. No new
preference service or framework.

Increase secondary-text readability, keep semantic desktop colors/system fonts,
use scoped inset focus outlines, consistent 28px pane/close targets and more
comfortable approval spacing. Explicit accessible action names and full-text
tooltips complement visual hierarchy.

## Usage and signatures

Existing `Sidebar::update` and `PaneWidget::update_meta` remain presentation
entry points. App gains a private `sync_desktop_preferences` operation and calls
it from settings notifications. The decision container becomes AdwWrapBox;
button signals, decision IDs and answer delivery retain their existing owner.

Pending-answer feedback, if verification demonstrates it is needed, belongs to
PaneWidget and is keyed by decision ID. App only starts/finishes the local
operation around its existing asynchronous IPC call. Completion cannot affect
a superseding decision or take focus from a different pane. No server policy
changes and no mutation retry.

## Alternatives and synthesis

Candidate A preserves three-line name/status/activity/location rows with a
separate close column and native wrapped options. Candidate B groups status with
activity and stacks all answers vertically. A is the base: it preserves useful
message space and terminal density. B contributes keeping preference policy
inside App and avoiding a new forwarding service. Always-stacked choices and
duplicated responsive layouts are rejected for unnecessary height/state cost.
The independent rubric/cross-review is recorded in `design-review.md`.

## Verification

Use a GTK allocation regression first: long question/three choices must fit a
320px card, render full text and deliver the selected original option; VTE
identity and contents survive a decision change. Capture a deterministic
populated real-app fixture before/after at desktop and narrow widths, light/dark,
high contrast and larger fonts. Inspect keyboard-focused inactive controls and
urgency beside close. Verify launch/runtime motion preference and pointer versus
keyboard feedback through native widget states.

Run existing reliability probes, display regressions, build, workspace tests,
clippy and fmt. Review code and motion against the invoked skills, update docs/06
and project verification instructions, commit by concern and push main.
