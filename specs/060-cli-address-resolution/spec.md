# Specification: CLI pane identity and working directories

Created: 2026-10-14. Branch: `t3/repo-bug-fixes`. Status: clarified.

Input: Audit the repository for bugs and submit verified fixes.

## User scenarios and testing

### Story 1: Keep an attach bound to its pane

Priority: P1. A user attaches by a pane name from spec 038. The CLI must
keep streaming and sending input to that pane until it exits or closes.

Acceptance:

1. Closing a pane attached by name terminates the CLI successfully.
2. Exiting the child process terminates an attach opened by name.
3. Renaming the pane and reusing its old name does not redirect input.
4. Attaching by opaque ID keeps the same behavior.

Independent test: run the real CLI in a controlling PTY against an isolated
server, then close, exit, or rename the attached pane.

### Story 2: Resolve explicit cwd in the caller's directory

Priority: P1. A user runs `new`, `pane spawn`, or `pane split` with `--cwd .`
from a different directory than the server. The new child must start in
the caller's directory.

Acceptance:

1. All three commands resolve explicit relative cwd before sending IPC.
2. Explicit absolute cwd keeps its meaning.
3. Omitting cwd retains the existing server and pane defaults.
4. Invalid cwd still fails without launching a child.

Independent test: invoke each CLI command from a temporary checkout with
`--cwd . -- pwd` and compare the real output with that checkout's path.

## Requirements and scope

- Resolve a pane name once and retain its opaque ID for the attach lifetime.
- Resolve explicit CLI cwd against the caller, using the existing task path policy.
- Use existing IPC methods, testkit isolation, and the native PTY boundary.
- Keep server defaults, GUI behavior, and the IPC schema unchanged.

No unresolved requirements or new architectural decisions.

## Success criteria

Both regression commands fail before their fixes and pass afterward.
`scripts/verify.sh full` passes before PR submission.
