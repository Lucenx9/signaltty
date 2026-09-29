# Codex pane runtime verification — 2026-09-30

## Reproduction
Installed Codex 0.159.1 TUI had the current live pane environment while the
existing detached app-server retained an obsolete pane id. Running the installed
UserPromptSubmit command with that inherited context and `{}` exited 1 with
NO_SUCH_PANE. No prompt contents or credentials were collected.

The real-PTY regression also reproduced a more subtle case: a valid stale pane
received Working while the current pane remained Unknown. Before the fix the
assertion failed; afterward separate launch and split panes independently reach
Working and Done using their actual installed semantic hook commands.

## Fix and checks
Direct managed local interactive Codex fresh/fork/resume uses native --no-daemon
when a bounded version probe confirms support. Original argv and hook commands
remain unchanged on disk. Ordinary shell invocation requires the same flag;
existing clients retain their runtime until explicitly exited/resumed.

- 211 workspace tests passed; 11 pre-existing display tests remain ignored.
  No GUI changed; prior automatic-hooks verification ran those native tests.
- Workspace debug/release builds and fmt passed. Clippy passed with only the
  two pre-existing audit.rs/router.rs warnings (no new warnings).
- Public PTY tests cover independent pane attribution, split, official resume
  after server restart, unchanged installed hooks, fallback on unsupported
  version, and bounded timeout cleanup of wrapper descendants.
- Argument tests cover resume/fork, native boolean options, attached short
  values, variadic images, option values, prompt separator, idempotence,
  utilities, remote endpoints and explicit notices for unknown options.
- Standards/spec reviews found argument-coverage and probe-descendant issues;
  both were reproduced red, corrected and reviewed green.

## Native verification boundary
Installed 0.159.1 accepts --no-daemon --version and resume --last --help. A
no-prompt native Codex smoke launch in an isolated Signaltty server stayed live;
personal hooks/config/auth bytes and the existing shared daemon were unchanged.
It did not emit SessionStart in the bare PTY without an interactive viewer.
Native lifecycle delivery is therefore not claimed by that smoke check; routing
is proved by installed-command PTY fixtures and runtime isolation by the pinned
upstream startup implementation. No model prompt was submitted and no hook-trust
bypass was used. Invalid pane reporting remains strict.

Publication: implementation [1a2d5ce](https://github.com/Lucenx9/signaltty/commit/1a2d5cee8268d26767bba665ddfdc0804dd8ae8c)
pushed to main; [CI 36647316905](https://github.com/Lucenx9/signaltty/actions/runs/36647316905)
passed. Runtime binaries rebuilt, but the existing user server was left running
to preserve live sessions. The automatic launch policy takes effect when that
server is next started; manual --no-daemon works with the current server too.
