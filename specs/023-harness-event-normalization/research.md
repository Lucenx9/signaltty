# Research

Decision: repair normalization at the installed OpenCode plugin boundary.
The existing adapter already maps session.error to Failed/Error and a notification.
V2 execution.failed carries sessionID and a structured error; mapping it to idle
loses the failure. V1 creation can put identity in properties.info.id.

Primary contracts inspected:
- [OpenCode 2.0.25 execution events](https://github.com/anomalyco/opencode/blob/v2.0.25/packages/schema/src/session-event.ts)
- [OpenCode V2 structured error](https://github.com/anomalyco/opencode/blob/v2.0.25/packages/schema/src/session-error.ts)
- [OpenCode V1 session schema](https://github.com/anomalyco/opencode/blob/v2.0.25/packages/schema/src/v1/session.ts)
- [OpenCode plugins](https://opencode.ai/v2/docs/build/plugins)

Alternative: add new Rust adapter event names. Rejected: normalization belongs
in the provider shim, which already translates V2 to existing events. No ADR
needed: retain ADR-0013's shared installer and existing adapter contract.

Behavioral testing requires JavaScript execution. Node is test tooling only;
the installed plugin itself uses the provider runtime and no extra package.

Grok noted that V2 uses error.message while V1 uses error.data.message; the V2 fixture now uses its own tagged structured-error shape, and V1 nested errors are covered separately. Both paths pass.

## Broader audit triage

Native Grok 4.7 reviewed adapters, installer and server launch/hook/resume paths.
The answered-approval Blocked lifecycle is being reproduced and fixed in this PR.
Other candidates are recorded for focused validation rather than treated as
proven defects here:

- Reviewed turns becoming unread on SessionEnd/process exit: compare the intentional
  exit contract with desired seen/unseen semantics before changing it.
- Official resume argv using a bare executable and flag-like named session IDs:
  reproduce with a real provider parser and explicit binary path before changing
  provider-specific argument contracts.
- `/proc` refresh choosing the highest-PID shell child rather than foreground job:
  needs a controlled PTY/process-group reproduction.
- 500ms Codex feature probe and store-lock latency: unsupported/timed-out fallback
  is explicitly covered by ADR-0014 and existing tests; distinguish that policy
  from a measured IPC responsiveness regression.

These candidates do not affect the reproduced event-normalization fixes.
