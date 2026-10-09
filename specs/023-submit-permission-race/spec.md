# Specification: Prompt submission preserves permission decisions

Created: 2026-10-08. Status: clarified.

An orchestrator must never answer a worker's permission request through the
delayed Enter belonging to a submitted prompt. This preserves cmux's inline
decision boundary and herdr's reliable agent prompting, with accurate native
feedback as required by docs/14. No new UI or animation is needed.

## User scenario and testing

As an orchestrator, I submit work while the worker remains able to ask the user
for permission. A permission arriving during paste must remain unanswered.
This is P1 because implicit Enter can commit a choice the user never made.
The independent test captures the worker PTY input before and after its hook.

## Requirements and acceptance

1. An idle worker receives bracketed paste. If a permission request or pending
   decision arrives before the delayed Enter, submission returns AGENT_BUSY
   without writing Enter. The already delivered paste cannot be undone.
2. Every automatic Enter checks the current decision state
   under the same store lock as their PTY write. A hook cannot insert a decision
   between that check and write.
3. Existing idle/done submission, plain input-required follow-ups, activity
   confirmation, fast completion and Enter retry remain supported.
4. A regression uses a real isolated PTY, records exact input bytes, and injects
   the permission hook only after the paste has been received.

5. Submit confirmation and background ready commits must preserve a decision
   that arrived before the task commit. Such a task remains `input_required`
   with `decision_required` evidence until the decision is answered.

6. A background first-submit refusal after paste is recoverable, not terminal.
   With a pending decision the task parks at `input_required` with that
   decision as evidence; otherwise it uses existing submit-unconfirmed recovery.

## Scope and clarification

The task is a correction to the existing submission contract in spec 018.
No IPC fields, snapshot changes, provider routing or visual changes. A permission
that the provider has not yet reported cannot be detected by this hook boundary.
No unresolved requirements remain.

## Success criteria

The new regression records no carriage return after the permission hook and
receives AGENT_BUSY with partial-delivery details. Existing submit tests and
the full repository verification pass on the pinned toolchain.
