# Verification — 2026-10-11

Implementation commit `f4b6d52534f013a8de0cc363ff76ddc15fc42b47`, based on
main `79e7f7e`. Final evidence/docs commit changes no executable source.

`scripts/verify.sh full` passed all 46 steps, including all 34 isolated native GTK
tests, workspace build/tests, clippy policy, architecture checks, server QA and
refresh benchmark. Authoritative run: `target/verification/full-fg7jc8in`;
revision was committed and clean at launch. See [summary](evidence/full-summary.json).
Earlier `full-5hngica3` also passed, before integration with updated main.

## Behavior proof

Test-first first-prompt generation failed before implementation
(`/tmp/signaltty-title-generation-red.log`), then passed. Title/status geometry
failed on the shared first line (`/tmp/signaltty-panel-red.log`). The enlarged
worker + Approval scene failed while chips shared the activity line
(`/tmp/signaltty-task-red.log`), then passed after assigning optional chips their
own line. These regressions pass in the authoritative gate.

Server tests verify opt-in/default manual behavior, malformed auto_name params,
nonblocking accepted hook replies, first-prompt-only generation, immutable handle,
manual-rename race, failed provider fallback and restart persistence. Unit tests
verify paired naming events, output validation, interrupted-state restore,
Claude tool-free structured output and cancellation cleanup of child processes
and the private directory. Provider binaries in tests are deterministic fixtures.

Local installed CLI help and read-only Codex MCP metadata were checked. An empty
MCP table did not disable configured servers; an explicit TOML inline table of
per-name enabled=false entries left zero enabled. No live paid model request
was made. Input is bounded to 8192 characters; output to 16 KiB; queue/execution
together to 60 seconds with two concurrent slots.

## Native visuals and independent review

Inspected native light/dark populated panel, long-title panel, Sans 18 task/Approval
scene, automatic dialog at desktop and 360px/Sans18, and manual dialog. Titles
and task labels deliberately ellipsize; urgency and dialog actions remain visible.
Independent native reviewer accepted the same final GUI sources in full-5hngica3.
Final main-based captures were inspected again and are saved in [evidence](evidence/).

Gemini 3.8 Flash High via Antigravity reviewed two scope046 rounds:
- Round 1: no actionable code defects; final full gate and refreshed captures pending.
- Round 2: accepted `f4b6d52`, checked the passed main-based gate and final captures.
  Found three stale ADR0030 documentation references after renumbering. All three
  now point to ADR0031. No blocking defects remained.

Task IDs end with `signaltty-046-gemini-review-round-1` and
`signaltty-046-gemini-review-round-2`, each delegated independently.

## Proof limits

Fake CLI tests do not certify live Codex/Claude account compatibility, model title
quality, or quota behavior. Generation uses the server environment's CLI default
account/model, can consume account quota, and requires an accepted first-prompt
hook; screen-only agents are unsupported. Xvfb renders do not certify a desktop
AT-SPI bus, screen readers, Wayland IME, on-screen keyboards or OS folder chooser.
Terminal text and generated names in screenshots are deterministic fixtures,
not validation output or a live provider transcript. Desktop dialog backgrounds
include the fixture's unavailable Changes response; that is not a production IPC
or provider failure. See docs/17-agent-development.md for those proof boundaries.
