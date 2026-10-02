# Repeat the Claude approval-channel probe

> Historical probe artifact. Prepared against commit `eaf8076` on 2026-10-02.
> The updated repository already implements native permission replies through
> [spec 013](../013-local-agent-workflows/spec.md) and
> [ADR-0015](../../docs/adr/0015-local-agent-workflows.md).
> This document records the probe design, not the current implementation contract.

Run from the repository root on Linux. This live experiment uses your existing
Claude login and model allowance. It starts two short Haiku sessions in temporary
directories and an isolated signaltty server. It installs no personal hooks.
Claude can create transcript and workspace trust records for those sessions.

1. Build the server:

   ```sh
   cargo build -p signaltty-server
   ```

2. Check the CLI and login:

   ```sh
   claude --version
   claude auth status
   ```

3. Run the live experiment:

   ```sh
   python3 specs/003-inline-approvals/probe.py
   ```

The controller creates a private temporary root and prints its path. It trusts
only the temporary directories it created. Two panes request the exact command
`printf approved > approval-proof.txt`. The hook waits for an external response;
the controller chooses allow for one pane and deny for the other.

Both result records must contain `passed: true`. Only the allow pane creates a
marker with content `approved`. The deny pane displays `Denied by PermissionRequest
hook`. Both hooks report their correct injected pane and socket. The probe closes
its panes and shuts down its server in cleanup. A timeout or missing observation
fails the experiment; inspect the printed root's `server.log`, case `screen.txt`,
and `results.json`.

The probe disables updates for its own processes using `DISABLE_UPDATES`.
The variable is documented in the
[Claude environment reference](https://code.claude.com/docs/en/env-vars).
The original runs observed versions 2.1.287 and 2.1.288; this rerun records the
currently installed version.

The response file stands in for the future GUI choice. The probe does not test
`decision.answer`, because that method and the GUI bar are not implemented.
See [probe-results.md](probe-results.md) for the interactive terminal and timeout
experiments, and [plan.md](probe-plan.md) for production acceptance checks.
