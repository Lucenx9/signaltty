# Specification: Native harness task interruptions

Created: 2026-10-16. Status: clarified.

After publishing the Antigravity fix, the user requested checking other harnesses
because some workers still block. Audit both automatic launch policy and blocked
state propagation. Existing approval choices and provider configuration remain
authoritative; this correction covers adapter-reported native interruptions.

## Requirements

- **FR-001**: An adapter-classified blocked worker without a structured decision
  parks its task at input_required with hook_blocked evidence. Initial submit
  reconciles a blocker reached before the background commit.
- **FR-002**: An explicit new prompt or a finished turn resolves only attributable
  interruptions. Tool hooks alone preserve the existing blocked-state guard,
  because overlapping subagent traffic does not prove a main approval was answered. A still-pending structured native decision remains authoritative;
  a delivered native answer resumes its task. Repeated blockers preserve state
  and do not duplicate task events. Accepted terminal results remain immutable.
- **FR-003**: Isolated real IPC regressions cover Claude permission and idle
  notifications, legacy Codex permission payloads, and initial/follow-up races.
  Existing native permission-answer and screen interruption tests stay green.
- **FR-004**: Record the audited support boundaries: automatic flags currently
  cover Codex, Claude and Antigravity; explicit worker options remain literal.
  Distinguish an unverified provider integration gap from a reproduced defect.

## Clarification and scope

The user's accepted policy inherits an already automatic parent choice for agy.
This follow-up diagnoses other harnesses without changing unrelated provider
permission policies or treating provider trust/authentication as tool approval.
No authenticated paid model turns or live user sessions are altered. Missing
concrete examples limit attribution of the user's other observed stalls.
